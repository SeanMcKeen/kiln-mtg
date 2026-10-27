//! State-based actions (CR 704, `docs/03 Architecture/Subsystems/State-Based Actions.md`).
//!
//! Checked whenever a player would receive priority (CR 117.5, 704.3). All
//! applicable actions are performed simultaneously as one event, then the
//! check repeats. The legend rule's choice (CR 704.5j) is a decision made
//! before the batch is performed.

use std::collections::BTreeMap;

use mtg_core::{ObjectId, PlayerId};
use serde::{Deserialize, Serialize};

use mtg_cards::ir::CounterKind;

use crate::card::Keywords;
use crate::decision::DecisionKind;
use crate::engine::{Game, Task};
use crate::events::Event;
use crate::state::{LossReason, ZoneId};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub(crate) struct SbaTask {
    started: bool,
    /// Legend-rule groups still to decide: (controller, same-name legendary permanents).
    groups: Vec<(PlayerId, Vec<ObjectId>)>,
    /// Legendary permanents chosen to stay.
    kept: Vec<ObjectId>,
}

/// Non-choice SBAs found by one check.
struct SbaChecks {
    /// Players who lose (CR 704.5a–c).
    losers: Vec<(PlayerId, LossReason)>,
    /// Creatures to put into the graveyard; `true` = destroyed (CR 704.5g–h) rather than put (CR 704.5f).
    dying: Vec<(ObjectId, bool)>,
    /// Tokens outside the battlefield (CR 704.5d).
    tokens: Vec<ObjectId>,
    /// Permanents with both +1/+1 and -1/-1 counters, and the pairs to remove (CR 704.5q).
    pairs: Vec<(ObjectId, u32)>,
    /// Auras attached illegally or not attached (CR 704.5m).
    auras: Vec<ObjectId>,
    /// Equipment and other objects to unattach (CR 704.5n, 704.5p).
    unattach: Vec<ObjectId>,
}

impl SbaChecks {
    fn any(&self) -> bool {
        !self.losers.is_empty()
            || !self.dying.is_empty()
            || !self.tokens.is_empty()
            || !self.pairs.is_empty()
            || !self.auras.is_empty()
            || !self.unattach.is_empty()
    }
}

impl Game {
    /// CR 704.5j: groups of two or more legendary permanents with the same name and controller.
    fn legend_groups(&self) -> Vec<(PlayerId, Vec<ObjectId>)> {
        let mut by: BTreeMap<(PlayerId, &str), Vec<ObjectId>> = BTreeMap::new();
        for &id in &self.battlefield {
            // CR 613: name and supertypes after the layers (a copy, CR 707.2).
            let (def, controller) = if self.layers_live() {
                let Some(c) = self.chars(id) else { continue };
                if !c.type_line().has_supertype(mtg_core::Supertype::Legendary) {
                    continue;
                }
                (self.def(c.def_id()), c.controller())
            } else {
                let Some(o) = self.obj(id) else { continue };
                let d = self.def(o.def);
                if !d.is_legendary() {
                    continue;
                }
                (d, o.controller)
            };
            by.entry((controller, def.name.as_str()))
                .or_default()
                .push(id);
        }
        by.into_iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|((p, _), v)| (p, v))
            .collect()
    }

    /// Non-choice SBAs that apply now.
    fn sba_checks(&self) -> SbaChecks {
        let mut losers = Vec::new();
        // A player who can't lose isn't a candidate, so the check settles.
        for p in self
            .players
            .iter()
            .filter(|p| p.in_game() && self.can_lose(p.id))
        {
            if p.life <= 0 {
                losers.push((p.id, LossReason::ZeroLife)); // CR 704.5a
            } else if p.drew_from_empty {
                losers.push((p.id, LossReason::DrewFromEmptyLibrary)); // CR 704.5b
            } else if p.poison >= 10 {
                losers.push((p.id, LossReason::Poison)); // CR 704.5c
            }
        }
        // CR 704.6c: 21 or more combat damage from one commander.
        if !self.cmd.is_empty() {
            for p in self.commander_damage_losers() {
                if self.in_game(p) && self.can_lose(p) && !losers.iter().any(|(q, _)| *q == p) {
                    losers.push((p, LossReason::CommanderDamage));
                }
            }
        }
        let mut dying = Vec::new();
        for &id in &self.battlefield {
            if !self.is_creature(id) {
                continue;
            }
            let Some(o) = self.obj(id) else { continue };
            let t = self.toughness(id);
            if t <= 0 {
                dying.push((id, false)); // CR 704.5f: put into the graveyard, not destroyed
            } else if (i64::from(o.damage) >= i64::from(t) || o.deathtouch_damage)
                && !self.keywords(id).has(Keywords::INDESTRUCTIBLE)
            {
                // CR 704.5g, 704.5h: destroyed; indestructible ignores these (CR 702.12b).
                dying.push((id, true));
            }
        }
        // CR 704.5i: a planeswalker with 0 loyalty goes to its owner's graveyard.
        for &id in &self.battlefield {
            if self
                .chars(id)
                .is_some_and(|c| c.is(mtg_core::CardType::Planeswalker))
                && self.counters_on(id, CounterKind::Loyalty) == 0
            {
                dying.push((id, false));
            }
        }
        let (auras, unattach) = if self.attachments.is_empty() && !self.pool.has_auras() {
            (Vec::new(), Vec::new())
        } else {
            self.attachment_sbas()
        };
        SbaChecks {
            losers,
            dying,
            tokens: self.tokens_off_battlefield(),
            pairs: self.counter_pairs(),
            auras,
            unattach,
        }
    }

    /// Whether any SBA would be performed now (used by cleanup, CR 514.3a).
    pub(crate) fn sba_pending(&self) -> bool {
        self.sba_checks().any()
            || !self.legend_groups().is_empty()
            || self.commander_offer_pending()
    }

    pub(crate) fn sba_step(&mut self, mut t: SbaTask) {
        if !t.started {
            t.started = true;
            t.groups = self.legend_groups();
            self.begin_commander_offers();
        }
        if let Some((p, candidates)) = t.groups.first().cloned() {
            self.tasks.push(Task::Sba(t));
            self.decide(p, DecisionKind::LegendRule { candidates }, "704.5j");
            return;
        }
        // CR 903.9a: the owner may put the commander into the command zone.
        if let Some((p, kind)) = self.next_commander_offer() {
            self.tasks.push(Task::Sba(t));
            self.decide(p, kind, "903.9a");
            return;
        }
        let to_command = self.take_accepted_offers();
        // Perform everything simultaneously (CR 704.3).
        let checks = self.sba_checks();
        let mut legend_out: Vec<ObjectId> = Vec::new();
        for (_, group) in self.legend_groups() {
            legend_out.extend(group.into_iter().filter(|id| !t.kept.contains(id)));
        }
        let performed = checks.any() || !legend_out.is_empty() || !to_command.is_empty();
        let SbaChecks {
            losers,
            dying,
            tokens,
            pairs,
            auras,
            unattach,
        } = checks;
        for id in unattach {
            self.unattach(id);
        }
        for id in to_command {
            self.move_object(id, ZoneId::Command, true);
        }
        // CR 704.5b looks at draws since the last check.
        for p in &mut self.players {
            p.drew_from_empty = false;
        }
        for id in tokens {
            self.cease_to_exist(id);
        }
        for (id, n) in pairs {
            self.remove_counters(id, CounterKind::PlusOne, n);
            self.remove_counters(id, CounterKind::MinusOne, n);
        }
        // Everything this check puts into graveyards leaves at once (CR 704.3, 603.10a).
        let mut leaving: Vec<ObjectId> = dying.iter().map(|(id, _)| *id).collect();
        leaving.extend(legend_out.iter().copied());
        leaving.extend(auras.iter().copied());
        self.begin_leaving(&leaving);
        for (id, destroyed) in dying {
            let Some(owner) = self.obj(id).map(|o| o.owner) else {
                continue;
            };
            if destroyed {
                self.log(Event::Destroyed { object: id });
            }
            self.move_object(id, ZoneId::Graveyard(owner), true);
        }
        for id in legend_out.into_iter().chain(auras) {
            if let Some(owner) = self.obj(id).map(|o| o.owner) {
                self.move_object(id, ZoneId::Graveyard(owner), true);
            }
        }
        for &id in &self.battlefield.clone() {
            if let Some(o) = self.objects.get_mut(id) {
                o.deathtouch_damage = false;
            }
        }
        self.end_leaving();
        if !losers.is_empty() {
            self.players_lose(&losers);
        }
        if performed && self.result.is_none() {
            // CR 704.3: check again.
            self.tasks.push(Task::Sba(SbaTask::default()));
        }
    }

    pub(crate) fn sba_answer(&mut self, mut t: SbaTask, keep: ObjectId) {
        t.kept.push(keep);
        t.groups.remove(0);
        self.tasks.push(Task::Sba(t));
    }
}
