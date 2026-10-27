//! Attachments: Auras (CR 303.4), Equipment (CR 301.5) and the state-based
//! actions for illegal attachments (CR 704.5m–p; M5 plan L9).
//!
//! Attachments live in `Game::attachments` as `(attached object, host,
//! bound)`. `bound` marks an Aura whose enchant ability now only allows the
//! object its own effect put onto the battlefield (Animate Dead). Entries for
//! objects that left the battlefield are kept until cleanup, so "enchanted
//! creature" still means something to a leaves-the-battlefield trigger
//! (last known information, CR 603.10).

use mtg_cards::ir::{TargetKind, ZoneName};
use mtg_core::{ObjectId, PlayerId};

use crate::engine::Game;
use crate::events::Event;
use crate::state::{Target, ZoneId};

impl Game {
    /// What `id` is attached to, if anything.
    pub(crate) fn attached_to(&self, id: ObjectId) -> Option<Target> {
        self.attachments
            .iter()
            .find(|(a, _, _)| *a == id)
            .map(|(_, h, _)| *h)
    }

    /// Whether `source` is attached to `host` ("enchanted/equipped creature").
    pub(crate) fn is_attached_to(&self, source: Option<ObjectId>, host: ObjectId) -> bool {
        source.is_some_and(|s| {
            self.attachments
                .iter()
                .any(|(a, h, _)| *a == s && *h == Target::Object(host))
        })
    }

    /// CR 701.3: attach `what` to `host` (moving it if it was attached elsewhere).
    pub(crate) fn attach(&mut self, what: ObjectId, host: Target, bound: bool) {
        self.attachments.retain(|(a, _, _)| *a != what);
        self.attachments.push((what, host, bound));
        self.bump_layers();
        self.log(Event::Attached {
            object: what,
            to: host,
        });
    }

    pub(crate) fn unattach(&mut self, what: ObjectId) {
        let before = self.attachments.len();
        self.attachments.retain(|(a, _, _)| *a != what);
        if self.attachments.len() != before {
            self.bump_layers();
        }
    }

    /// Cleanup: forget attachments of objects that no longer exist.
    pub(crate) fn prune_attachments(&mut self) {
        let objects = &self.objects;
        self.attachments
            .retain(|(a, _, _)| objects.get(*a).is_some());
    }

    fn has_subtype(&self, id: ObjectId, sub: &str) -> bool {
        self.chars(id)
            .is_some_and(|c| c.type_line().has_subtype(sub))
    }

    /// Whether Aura `aura` may be attached to `host` now (CR 303.4c, 702.5a,
    /// 702.16c).
    fn aura_legal(&self, aura: ObjectId, host: Target, bound: bool) -> bool {
        let Some(c) = self.chars(aura) else {
            return false;
        };
        let controller = c.controller();
        let def = self.def(c.def_id());
        match host {
            Target::Player(p) => {
                matches!(&def.enchant, Some(spec) if matches!(&spec.what,
                    TargetKind::Player(rel) if self.player_rel(*rel, controller, p)))
                    && self.in_game(p)
            }
            Target::StackAbility { .. } => false,
            Target::Object(h) => {
                let Some(o) = self.obj(h) else { return false };
                // CR 702.16c: not by an Aura with a quality it has protection from.
                if o.zone == ZoneId::Battlefield && self.protected_from(h, def, controller) {
                    return false;
                }
                if bound {
                    return o.zone == ZoneId::Battlefield;
                }
                match def.enchant.as_ref().map(|s| &s.what) {
                    Some(TargetKind::Object(f)) => {
                        let zone_ok = match f.zone {
                            None | Some(ZoneName::Battlefield) => o.zone == ZoneId::Battlefield,
                            Some(ZoneName::Graveyard) => matches!(o.zone, ZoneId::Graveyard(_)),
                            Some(_) => false,
                        };
                        zone_ok && self.matches_filter(h, f, controller, Some(aura))
                    }
                    Some(TargetKind::Any) => o.zone == ZoneId::Battlefield,
                    _ => false,
                }
            }
        }
    }

    /// CR 704.5m–p: Auras to put into graveyards, and objects to unattach.
    pub(crate) fn attachment_sbas(&self) -> (Vec<ObjectId>, Vec<ObjectId>) {
        let mut auras = Vec::new();
        let mut unattach = Vec::new();
        for &id in &self.battlefield {
            if !self.has_subtype(id, "Aura") {
                continue;
            }
            match self.attachments.iter().find(|(a, _, _)| *a == id) {
                // CR 704.5m: attached illegally, or not attached.
                Some(&(_, host, bound)) if self.aura_legal(id, host, bound) => {}
                _ => auras.push(id),
            }
        }
        for &(a, host, _) in &self.attachments {
            if self.obj(a).is_none_or(|o| o.zone != ZoneId::Battlefield)
                || self.has_subtype(a, "Aura")
            {
                continue;
            }
            let legal = match host {
                // CR 704.5n: Equipment (not itself a creature, CR 301.5c)
                // attached to a creature that isn't protected from it.
                Target::Object(h) if self.has_subtype(a, "Equipment") => {
                    !self.is_creature(a)
                        && self.obj(h).is_some_and(|o| o.zone == ZoneId::Battlefield)
                        && self.is_creature(h)
                        && self.chars(a).is_some_and(|c| {
                            !self.protected_from(h, self.def(c.def_id()), c.controller())
                        })
                }
                // CR 704.5p: anything else attached becomes unattached.
                _ => false,
            };
            if !legal {
                unattach.push(a);
            }
        }
        (auras, unattach)
    }

    /// Effect::Attach: attach the source to `host` if it can be (CR 701.3b).
    pub(crate) fn attach_source(&mut self, source: ObjectId, host: ObjectId) {
        if self
            .obj(source)
            .is_none_or(|o| o.zone != ZoneId::Battlefield)
            || self.obj(host).is_none_or(|o| o.zone != ZoneId::Battlefield)
        {
            return;
        }
        if self.has_subtype(source, "Equipment")
            && (!self.is_creature(host) || self.is_creature(source))
        {
            return;
        }
        self.attach(source, Target::Object(host), false);
    }

    /// Animate Dead: the card the source enchants returns to the battlefield
    /// under `controller`'s control, and the source is attached to it.
    pub(crate) fn return_attached(&mut self, source: ObjectId, controller: PlayerId) {
        if self
            .obj(source)
            .is_none_or(|o| o.zone != ZoneId::Battlefield)
        {
            return;
        }
        let Some(Target::Object(card)) = self.attached_to(source) else {
            return;
        };
        if self
            .obj(card)
            .is_none_or(|o| !matches!(o.zone, ZoneId::Graveyard(_)))
        {
            return;
        }
        if let Some(new) = self.move_object(card, ZoneId::Battlefield, true) {
            self.set_controller(new, controller);
            self.attach(source, Target::Object(new), true);
        }
    }

    // ---- phasing (CR 702.26) -------------------------------------------------

    /// CR 702.26b: `id` phases out, along with what's attached to it
    /// (CR 702.26g). No zone change happens (CR 702.26d).
    pub(crate) fn phase_out(&mut self, id: ObjectId) {
        let Some(o) = self.obj(id).copied() else {
            return;
        };
        if o.zone != ZoneId::Battlefield || !self.battlefield.contains(&id) {
            return;
        }
        self.battlefield.retain(|x| *x != id);
        self.phased_out.push((id, o.controller));
        if let Some(c) = &mut self.combat {
            c.remove(id);
        }
        self.bump_layers();
        let attached: Vec<ObjectId> = self
            .attachments
            .iter()
            .filter(|(_, h, _)| *h == Target::Object(id))
            .map(|(a, _, _)| *a)
            .collect();
        for a in attached {
            self.phase_out(a);
        }
    }

    /// CR 502.1: permanents `p` controlled as they phased out phase in.
    pub(crate) fn phase_in(&mut self, p: PlayerId) {
        if self.phased_out.is_empty() {
            return;
        }
        let back: Vec<ObjectId> = self
            .phased_out
            .iter()
            .filter(|(_, c)| *c == p)
            .map(|(id, _)| *id)
            .collect();
        self.phased_out.retain(|(_, c)| *c != p);
        for id in back {
            if self.obj(id).is_some_and(|o| o.zone == ZoneId::Battlefield) {
                self.battlefield.push(id);
            }
        }
        self.bump_layers();
    }

    /// Whether `p` is under a player rule now.
    /// Whether `p` is under `rule`, from an effect or a static ability
    /// (Platinum Angel).
    pub(crate) fn player_has_rule(&self, p: PlayerId, rule: mtg_cards::ir::PlayerRuleKind) -> bool {
        self.has_player_rule(p, rule)
            || (self.pool.has_rule_statics()
                && self.active_rule_statics().iter().any(|(_, controller, e)| {
                    matches!(e, mtg_cards::ir::StaticEffect::PlayerRule { who, rule: r }
                        if *r == rule && self.player_rel(*who, *controller, p))
                }))
    }

    pub(crate) fn has_player_rule(&self, p: PlayerId, rule: mtg_cards::ir::PlayerRuleKind) -> bool {
        (!self.player_rules.is_empty()
            && self
                .player_rules
                .iter()
                .any(|(q, r, _)| *q == p && *r == rule))
            || self
                .player_rules_eot
                .iter()
                .any(|(q, r)| *q == p && *r == rule)
    }
}
