//! Combat (CR 506–511, `docs/03 Architecture/Subsystems/Combat.md`).
//!
//! Declarations are factored one creature at a time (R1). Blocks stay
//! constraint-aware for menace (M2 plan, D8). Damage is assigned one point at
//! a time when the controller has a real choice (CR 510.1c, 702.19b).

use mtg_core::{ObjectId, PlayerId};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::card::Keywords;
use crate::decision::{DamageRecipient, DecisionKind};
use crate::engine::{Game, Task};
use crate::events::Event;
use crate::state::{Attacker, Target, ZoneId};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct AttackTask {
    candidates: Vec<ObjectId>,
    next: usize,
    declared: Vec<(ObjectId, PlayerId, Option<ObjectId>)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct BlockTask {
    defenders: SmallVec<[PlayerId; 8]>,
    defender: usize,
    candidates: Vec<ObjectId>,
    next: usize,
    started: bool,
    /// `(blocker, attacker)` chosen so far, all defenders.
    chosen: Vec<(ObjectId, ObjectId)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct DamageTask {
    participants: Vec<ObjectId>,
    /// Index into the attacking participants being assigned.
    idx: usize,
    /// Points left for the current attacker, once started.
    remaining: Option<u32>,
    assignments: Vec<(ObjectId, DamageRecipient, u32)>,
}

impl Game {
    // ---- declare attackers (CR 508.1) -------------------------------------

    pub(crate) fn attack_task(&self) -> AttackTask {
        let active = self.turn.active;
        let has_opponent = self.players.iter().any(|p| p.in_game() && p.id != active);
        // "Creatures can't attack this turn" (Orim's Chant, kicked).
        let barred =
            self.has_player_rule(active, mtg_cards::ir::PlayerRuleKind::CreaturesCantAttack);
        let candidates = if self.in_game(active) && has_opponent && !barred {
            self.battlefield
                .iter()
                .copied()
                .filter(|&id| {
                    let Some(o) = self.obj(id) else { return false };
                    let kw = self.keywords(id);
                    // CR 508.1a: untapped; haste or continuous control since the turn began (CR 302.6); CR 702.3b defender.
                    o.controller == active
                        && self.is_creature(id)
                        && !o.tapped
                        && (kw.has(Keywords::HASTE) || !self.is_summoning_sick(id))
                        && !kw.has(Keywords::DEFENDER)
                })
                .collect()
        } else {
            Vec::new()
        };
        AttackTask {
            candidates,
            next: 0,
            declared: Vec::new(),
        }
    }

    pub(crate) fn attack_step(&mut self, t: AttackTask) {
        let active = self.turn.active;
        if let Some(&creature) = t.candidates.get(t.next) {
            // CR 802.3, 903.2: each attacker chooses which defending player it attacks.
            let mut targets: Vec<Option<PlayerId>> = vec![None];
            targets.extend(
                self.turn_order_from(active)
                    .into_iter()
                    .filter(|p| *p != active)
                    .map(Some),
            );
            // CR 508.1b: or a planeswalker an opponent controls.
            let planeswalkers: Vec<ObjectId> = self
                .battlefield
                .iter()
                .copied()
                .filter(|id| {
                    self.obj(*id).is_some_and(|o| o.controller != active)
                        && self
                            .chars(*id)
                            .is_some_and(|c| c.is(mtg_core::CardType::Planeswalker))
                })
                .collect();
            self.tasks.push(Task::DeclareAttackers(t));
            self.decide(
                active,
                DecisionKind::DeclareAttacker {
                    creature,
                    targets,
                    planeswalkers,
                },
                "508.1a",
            );
            return;
        }
        let mut attackers = Vec::new();
        for (id, defending, planeswalker) in t.declared {
            // CR 508.1f: attacking creatures become tapped, except with vigilance (CR 702.20b).
            if !self.keywords(id).has(Keywords::VIGILANCE) {
                self.tap(id);
            }
            self.log(Event::AttackerDeclared {
                object: id,
                defending,
            });
            attackers.push(Attacker {
                object: id,
                defending,
                planeswalker,
                blocked: false,
                blockers: SmallVec::new(),
            });
        }
        if let Some(c) = &mut self.combat {
            c.attackers = attackers;
            c.declared_attackers = true;
        }
    }

    pub(crate) fn attack_answer(
        &mut self,
        mut t: AttackTask,
        target: Option<(PlayerId, Option<ObjectId>)>,
    ) {
        if let Some((p, pw)) = target {
            t.declared.push((t.candidates[t.next], p, pw));
        }
        t.next += 1;
        self.tasks.push(Task::DeclareAttackers(t));
    }

    // ---- declare blockers (CR 509.1) --------------------------------------

    pub(crate) fn block_task(&self) -> BlockTask {
        let attacked: SmallVec<[PlayerId; 8]> = self
            .combat
            .as_ref()
            .map(|c| c.attackers.iter().map(|a| a.defending).collect())
            .unwrap_or_default();
        // CR 802.4: each defending player declares blockers in APNAP order; their
        // blocks are judged ignoring other players' attackers (CR 802.4b).
        let defenders = self
            .turn_order_from(self.next_in_game(self.turn.active))
            .into_iter()
            .filter(|p| *p != self.turn.active && attacked.contains(p))
            .collect();
        BlockTask {
            defenders,
            defender: 0,
            candidates: Vec::new(),
            next: 0,
            started: false,
            chosen: Vec::new(),
        }
    }

    /// Attackers attacking `d`.
    fn attackers_of(&self, d: PlayerId) -> Vec<ObjectId> {
        self.combat
            .as_ref()
            .map(|c| {
                c.attackers
                    .iter()
                    .filter(|a| a.defending == d)
                    .map(|a| a.object)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// CR 702.9b, 702.17b: flying attackers can be blocked only by flying or
    /// reach; CR 702.16f: not by creatures with a quality it has protection from.
    fn can_block(&self, blocker: ObjectId, attacker: ObjectId) -> bool {
        let a = self.keywords(attacker);
        let b = self.keywords(blocker);
        (!a.has(Keywords::FLYING) || b.has(Keywords::FLYING) || b.has(Keywords::REACH))
            && !self
                .obj(blocker)
                .is_some_and(|o| self.protected_from(attacker, self.def(o.def), o.controller))
    }

    /// Whether, after `chosen`, the remaining candidates can still satisfy
    /// every menace attacker that has exactly one blocker (CR 702.111b).
    fn menace_feasible(
        &self,
        attackers: &[ObjectId],
        chosen: &[(ObjectId, ObjectId)],
        remaining: &[ObjectId],
    ) -> bool {
        let deficits: Vec<ObjectId> = attackers
            .iter()
            .copied()
            .filter(|a| {
                self.keywords(*a).has(Keywords::MENACE)
                    && chosen.iter().filter(|(_, x)| x == a).count() == 1
            })
            .collect();
        if deficits.len() > remaining.len() {
            return false;
        }
        // Match each deficit attacker to a distinct remaining candidate that can block it.
        let mut owner: Vec<Option<usize>> = vec![None; remaining.len()];
        for d in 0..deficits.len() {
            let mut seen = vec![false; remaining.len()];
            if !self.match_deficit(d, &deficits, remaining, &mut owner, &mut seen) {
                return false;
            }
        }
        true
    }

    fn match_deficit(
        &self,
        d: usize,
        deficits: &[ObjectId],
        remaining: &[ObjectId],
        owner: &mut [Option<usize>],
        seen: &mut [bool],
    ) -> bool {
        for (c, &cand) in remaining.iter().enumerate() {
            if seen[c] || !self.can_block(cand, deficits[d]) {
                continue;
            }
            seen[c] = true;
            if owner[c].is_none_or(|o| self.match_deficit(o, deficits, remaining, owner, seen)) {
                owner[c] = Some(d);
                return true;
            }
        }
        false
    }

    pub(crate) fn block_step(&mut self, mut t: BlockTask) {
        while let Some(&d) = t.defenders.get(t.defender) {
            if !t.started {
                // CR 509.1a: untapped creatures the defending player controls.
                t.candidates = self
                    .battlefield
                    .iter()
                    .copied()
                    .filter(|&id| {
                        self.obj(id).is_some_and(|o| o.controller == d && !o.tapped)
                            && self.is_creature(id)
                    })
                    .collect();
                t.next = 0;
                t.started = true;
            }
            let attackers = self.attackers_of(d);
            while let Some(&c) = t.candidates.get(t.next) {
                let rest = &t.candidates[t.next + 1..];
                let mut options: Vec<Option<ObjectId>> = Vec::new();
                if self.menace_feasible(&attackers, &t.chosen, rest) {
                    options.push(None);
                }
                for &a in &attackers {
                    if !self.can_block(c, a) {
                        continue;
                    }
                    let mut trial = t.chosen.clone();
                    trial.push((c, a));
                    if self.menace_feasible(&attackers, &trial, rest) {
                        options.push(Some(a));
                    }
                }
                debug_assert!(
                    !options.is_empty(),
                    "menace feasibility always leaves an option"
                );
                if options.len() == 1 && options[0].is_none() {
                    // No legal block exists for this creature: not a choice.
                    t.next += 1;
                    continue;
                }
                // When not blocking would strand a menace attacker, option 0 isn't `None`.
                self.tasks.push(Task::DeclareBlockers(t));
                self.decide(
                    d,
                    DecisionKind::DeclareBlocker {
                        creature: c,
                        attackers: options,
                    },
                    "509.1a",
                );
                return;
            }
            t.defender += 1;
            t.started = false;
        }
        // CR 509.1h: attackers with blockers become blocked.
        let chosen = std::mem::take(&mut t.chosen);
        for &(b, a) in &chosen {
            self.log(Event::BlockerDeclared {
                blocker: b,
                attacker: a,
            });
        }
        if let Some(c) = &mut self.combat {
            for (b, a) in chosen {
                c.blocks.push((b, a));
                if let Some(att) = c.attackers.iter_mut().find(|x| x.object == a) {
                    att.blockers.push(b);
                }
            }
            for att in &mut c.attackers {
                att.blocked = !att.blockers.is_empty();
            }
        }
    }

    pub(crate) fn block_answer(&mut self, mut t: BlockTask, choice: Option<ObjectId>) {
        if let Some(a) = choice {
            t.chosen.push((t.candidates[t.next], a));
        }
        t.next += 1;
        self.tasks.push(Task::DeclareBlockers(t));
    }

    // ---- combat damage (CR 510) -------------------------------------------

    pub(crate) fn any_first_striker(&self) -> bool {
        self.combatants().iter().any(|id| {
            let k = self.keywords(*id);
            k.has(Keywords::FIRST_STRIKE) || k.has(Keywords::DOUBLE_STRIKE)
        })
    }

    /// Attackers then blockers currently in combat.
    fn combatants(&self) -> Vec<ObjectId> {
        let Some(c) = &self.combat else {
            return Vec::new();
        };
        let mut v: Vec<ObjectId> = c.attackers.iter().map(|a| a.object).collect();
        v.extend(c.blocks.iter().map(|(b, _)| *b));
        v
    }

    pub(crate) fn damage_task(&mut self, first_strike: bool) -> DamageTask {
        let all = self.combatants();
        let participants = if first_strike {
            // CR 510.4 / 702.4b: only first and double strikers deal damage in the first step.
            let fs: Vec<ObjectId> = all
                .iter()
                .copied()
                .filter(|id| {
                    let k = self.keywords(*id);
                    k.has(Keywords::FIRST_STRIKE) || k.has(Keywords::DOUBLE_STRIKE)
                })
                .collect();
            if let Some(c) = &mut self.combat {
                c.first_strikers.clone_from(&fs);
                c.had_first_strike_step = true;
            }
            fs
        } else if self
            .combat
            .as_ref()
            .is_some_and(|c| c.had_first_strike_step)
        {
            let fs = self
                .combat
                .as_ref()
                .map(|c| c.first_strikers.clone())
                .unwrap_or_default();
            all.into_iter()
                .filter(|id| !fs.contains(id) || self.keywords(*id).has(Keywords::DOUBLE_STRIKE))
                .collect()
        } else {
            all
        };
        DamageTask {
            participants,
            idx: 0,
            remaining: None,
            assignments: Vec::new(),
        }
    }

    fn assigned_to(&self, t: &DamageTask, b: ObjectId) -> u32 {
        t.assignments
            .iter()
            .filter(|(_, r, _)| *r == DamageRecipient::Creature(b))
            .map(|(_, _, n)| n)
            .sum()
    }

    /// CR 702.19b with 702.2c: lethal damage has been assigned to blocker `b`.
    fn lethal_assigned(&self, t: &DamageTask, source: ObjectId, b: ObjectId) -> bool {
        let marked = self.obj(b).map_or(0, |o| o.damage);
        let left =
            i64::from(self.toughness(b)) - i64::from(marked) - i64::from(self.assigned_to(t, b));
        if left <= 0 {
            return true;
        }
        self.keywords(source).has(Keywords::DEATHTOUCH)
            && t.assignments
                .iter()
                .any(|(s, r, n)| *s == source && *r == DamageRecipient::Creature(b) && *n > 0)
    }

    fn add_assignment(t: &mut DamageTask, source: ObjectId, r: DamageRecipient, n: u32) {
        if n == 0 {
            return;
        }
        if let Some(e) = t
            .assignments
            .iter_mut()
            .find(|(s, x, _)| *s == source && *x == r)
        {
            e.2 += n;
        } else {
            t.assignments.push((source, r, n));
        }
    }

    pub(crate) fn damage_step(&mut self, mut t: DamageTask) {
        let attacking: Vec<ObjectId> = t
            .participants
            .iter()
            .copied()
            .filter(|id| {
                self.combat
                    .as_ref()
                    .is_some_and(|c| c.attacker(*id).is_some())
            })
            .collect();
        while let Some(&a) = attacking.get(t.idx) {
            let Some(att) = self.combat.as_ref().and_then(|c| c.attacker(a)).cloned() else {
                t.idx += 1;
                continue;
            };
            let power = u32::try_from(self.power(a).max(0)).unwrap_or(0);
            let remaining = t.remaining.unwrap_or(power);
            // CR 510.1a: creatures assigning 0 or less assign no combat damage.
            if remaining == 0 || !self.obj(a).is_some_and(|o| o.zone == ZoneId::Battlefield) {
                t.idx += 1;
                t.remaining = None;
                continue;
            }
            let trample = self.keywords(a).has(Keywords::TRAMPLE);
            // An attacked planeswalker takes the damage (if it's still there, CR 506.4).
            let player = match att.planeswalker {
                Some(pw) => DamageRecipient::Creature(pw),
                None => DamageRecipient::Player(att.defending),
            };
            // CR 800.4e: no combat damage is assigned to a player who has left.
            let player_ok = match att.planeswalker {
                Some(pw) => self.on_battlefield(pw),
                None => self.in_game(att.defending),
            };
            let blockers: Vec<ObjectId> = att
                .blockers
                .iter()
                .copied()
                .filter(|b| self.obj(*b).is_some_and(|o| o.zone == ZoneId::Battlefield))
                .collect();
            if !att.blocked || (blockers.is_empty() && trample) {
                // CR 510.1b; CR 702.19d: a blocked trampler with no blockers left assigns to the player.
                if player_ok {
                    Self::add_assignment(&mut t, a, player, remaining);
                }
                t.idx += 1;
                t.remaining = None;
                continue;
            }
            if blockers.is_empty() {
                // CR 510.1c: blocked, no blockers remain, no trample: no damage.
                t.idx += 1;
                t.remaining = None;
                continue;
            }
            if blockers.len() == 1 && !trample {
                Self::add_assignment(&mut t, a, DamageRecipient::Creature(blockers[0]), remaining);
                t.idx += 1;
                t.remaining = None;
                continue;
            }
            // Point-by-point division (CR 510.1c, 702.19b).
            let all_lethal = blockers.iter().all(|b| self.lethal_assigned(&t, a, *b));
            let mut recipients: Vec<DamageRecipient> = blockers
                .iter()
                .map(|b| DamageRecipient::Creature(*b))
                .collect();
            if trample && all_lethal && player_ok {
                recipients.push(player);
            }
            if recipients.len() == 1 {
                Self::add_assignment(&mut t, a, recipients[0], 1);
                t.remaining = Some(remaining - 1);
                if remaining == 1 {
                    t.idx += 1;
                    t.remaining = None;
                }
                continue;
            }
            t.remaining = Some(remaining);
            let controller = self.obj(a).map_or(self.turn.active, |o| o.controller);
            self.tasks.push(Task::CombatDamage(t));
            self.decide(
                controller,
                DecisionKind::AssignDamage {
                    source: a,
                    remaining,
                    recipients,
                },
                "510.1c",
            );
            return;
        }
        // CR 510.1d: each blocker assigns its damage to the attacker it blocks.
        let blocks = self
            .combat
            .as_ref()
            .map(|c| c.blocks.clone())
            .unwrap_or_default();
        for (b, a) in blocks {
            if !t.participants.contains(&b)
                || !self.obj(a).is_some_and(|o| o.zone == ZoneId::Battlefield)
            {
                continue;
            }
            let power = u32::try_from(self.power(b).max(0)).unwrap_or(0);
            Self::add_assignment(&mut t, b, DamageRecipient::Creature(a), power);
        }
        // CR 510.2: all assigned combat damage is dealt simultaneously.
        for (source, r, n) in std::mem::take(&mut t.assignments) {
            let target = match r {
                DamageRecipient::Creature(id) => Target::Object(id),
                DamageRecipient::Player(p) => Target::Player(p),
            };
            self.deal_damage(source, target, n, true);
        }
    }

    pub(crate) fn damage_answer(&mut self, mut t: DamageTask, r: DamageRecipient) {
        let attacking: Vec<ObjectId> = t
            .participants
            .iter()
            .copied()
            .filter(|id| {
                self.combat
                    .as_ref()
                    .is_some_and(|c| c.attacker(*id).is_some())
            })
            .collect();
        let a = attacking[t.idx];
        Self::add_assignment(&mut t, a, r, 1);
        let left = t.remaining.unwrap_or(1) - 1;
        if left == 0 {
            t.idx += 1;
            t.remaining = None;
        } else {
            t.remaining = Some(left);
        }
        self.tasks.push(Task::CombatDamage(t));
    }
}
