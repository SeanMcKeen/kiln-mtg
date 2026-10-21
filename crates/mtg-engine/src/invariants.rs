//! Engine invariants (`docs/08 Testing/Invariants & Property Tests.md`).
//!
//! Checked after every `apply` in fuzzing and tests. A violation is an engine
//! bug; the message says which invariant broke.

use std::collections::BTreeMap;

use mtg_core::ObjectId;

use crate::decision::DecisionKind;
use crate::engine::Game;
use crate::state::ZoneId;

impl Game {
    /// Checks every invariant. `deck_sizes` gives each seat's card count for
    /// the conservation check (no tokens or copies exist in M2).
    pub fn check_invariants(&self, deck_sizes: &[usize]) -> Result<(), String> {
        // 1. Every object is in exactly one zone, matching its `zone` field.
        let mut seen: BTreeMap<ObjectId, ZoneId> = BTreeMap::new();
        let mut note = |id: ObjectId, z: ZoneId| -> Result<(), String> {
            if let Some(prev) = seen.insert(id, z) {
                return Err(format!("invariant 1: {id} is in {prev:?} and {z:?}"));
            }
            Ok(())
        };
        for p in &self.players {
            for (list, z) in [
                (&p.library, ZoneId::Library(p.id)),
                (&p.hand, ZoneId::Hand(p.id)),
                (&p.graveyard, ZoneId::Graveyard(p.id)),
            ] {
                for &id in list {
                    note(id, z)?;
                }
            }
        }
        for &id in &self.battlefield {
            note(id, ZoneId::Battlefield)?;
        }
        for &id in &self.exile {
            note(id, ZoneId::Exile)?;
        }
        for s in &self.stack {
            note(s.object, ZoneId::Stack)?;
        }
        for (id, o) in self.objects.iter() {
            match seen.get(&id) {
                Some(z) if *z == o.zone => {}
                Some(z) => {
                    return Err(format!(
                        "invariant 1: {id} says zone {:?} but is listed in {z:?}",
                        o.zone
                    ));
                }
                None => {
                    return Err(format!(
                        "invariant 1: {id} ({:?}) is in no zone list (dangling)",
                        o.zone
                    ));
                }
            }
            if let ZoneId::Library(p) | ZoneId::Hand(p) | ZoneId::Graveyard(p) = o.zone
                && o.owner != p
            {
                return Err(format!(
                    "invariant 1: {id} owned by {} is in {p}'s {:?}",
                    o.owner, o.zone
                ));
            }
        }
        if seen.len() != self.objects.live_count() {
            return Err(format!(
                "invariant 1: {} listed ids vs {} live objects",
                seen.len(),
                self.objects.live_count()
            ));
        }

        // 2. The pending decision is well-formed.
        match (&self.pending, &self.result) {
            (Some(d), None) => {
                if d.option_count() == 0 {
                    return Err(format!("invariant 2: decision {:?} has no options", d.id));
                }
                if !self.in_game(d.player) {
                    return Err(format!(
                        "invariant 2: decision {:?} goes to {}, who has left the game",
                        d.id, d.player
                    ));
                }
                if self.tasks.is_empty() {
                    return Err("invariant 2: decision pending with an empty task stack".into());
                }
                // 7. SBA fixpoint: nothing applies while a player holds priority (CR 704.3).
                if matches!(d.kind, DecisionKind::Priority { .. }) && self.sba_pending() {
                    return Err(
                        "invariant 7: a state-based action applies while a player has priority"
                            .into(),
                    );
                }
            }
            (None, Some(_)) => {}
            (Some(_), Some(_)) => {
                return Err("invariant 2: decision pending after the game ended".into());
            }
            (None, None) => return Err("invariant 2: no decision and no result".into()),
        }

        // 6. Conservation: each in-game player owns exactly their deck's cards.
        for p in self.players.iter().filter(|p| p.in_game()) {
            let owned = self.objects.iter().filter(|(_, o)| o.owner == p.id).count();
            if let Some(&n) = deck_sizes.get(p.id.0 as usize)
                && owned != n
            {
                return Err(format!(
                    "invariant 6: {} owns {owned} objects, deck had {n}",
                    p.id
                ));
            }
        }

        // Stack items refer to live objects on the stack, and so on.
        for s in &self.stack {
            if self.obj(s.object).is_none_or(|o| o.zone != ZoneId::Stack) {
                return Err(format!("stack item {} isn't a live stack object", s.object));
            }
        }
        if let Some(c) = &self.combat {
            for a in &c.attackers {
                if self
                    .obj(a.object)
                    .is_none_or(|o| o.zone != ZoneId::Battlefield)
                {
                    return Err(format!("attacker {} isn't on the battlefield", a.object));
                }
            }
        }
        Ok(())
    }
}
