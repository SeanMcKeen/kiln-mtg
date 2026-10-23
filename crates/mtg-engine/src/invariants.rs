//! Engine invariants (`docs/08 Testing/Invariants & Property Tests.md`).
//!
//! Checked after every `apply` in fuzzing and tests. A violation is an engine
//! bug; the message says which invariant broke.

use mtg_core::ObjectId;

use crate::decision::DecisionKind;
use crate::engine::Game;
use crate::state::ZoneId;

impl Game {
    /// Checks every invariant. `deck_sizes` gives each seat's card count for
    /// the conservation check (no tokens or copies exist in M2).
    pub fn check_invariants(&self, deck_sizes: &[usize]) -> Result<(), String> {
        // 1. Every object is in exactly one zone, matching its `zone` field.
        // Indexed by arena slot (cheap enough to run after every decision).
        let mut seen: Vec<Option<(ObjectId, ZoneId)>> = vec![None; self.objects.slot_count()];
        let mut listed = 0usize;
        let mut note = |id: ObjectId, z: ZoneId| -> Result<(), String> {
            let Some(slot) = seen.get_mut(id.index as usize) else {
                return Err(format!(
                    "invariant 1: {id} is listed in {z:?} but has no arena slot"
                ));
            };
            if let Some((_, prev)) = slot {
                return Err(format!("invariant 1: {id} is in {prev:?} and {z:?}"));
            }
            *slot = Some((id, z));
            listed += 1;
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
        // Phased-out permanents are still on the battlefield (CR 702.26d).
        for &(id, _) in &self.phased_out {
            note(id, ZoneId::Battlefield)?;
        }
        for &id in &self.exile {
            note(id, ZoneId::Exile)?;
        }
        for &id in &self.cmd.zone {
            note(id, ZoneId::Command)?;
        }
        // Abilities on the stack aren't objects in a zone; their `object` is the source.
        for s in self.stack.iter().filter(|s| s.is_spell()) {
            note(s.object, ZoneId::Stack)?;
        }
        let mut owned = [0usize; 8];
        for (id, o) in self.objects.iter() {
            // Tokens (CR 111.1) and copies of cards (CR 707.12) aren't deck cards.
            if !self.def(o.def).token
                && !self.copies.contains(&id)
                && let Some(n) = owned.get_mut(o.owner.0 as usize)
            {
                *n += 1;
            }
            match seen.get(id.index as usize).copied().flatten() {
                Some((listed_id, z)) if listed_id == id && z == o.zone => {}
                Some((listed_id, _)) if listed_id != id => {
                    return Err(format!(
                        "invariant 1: zone lists hold stale id {listed_id}, live object is {id}"
                    ));
                }
                Some((_, z)) => {
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
        if listed != self.objects.live_count() {
            return Err(format!(
                "invariant 1: {listed} listed ids vs {} live objects",
                self.objects.live_count()
            ));
        }

        // 2. The pending decision is well-formed.
        match (&self.pending, &self.result) {
            (Some(d), None) => {
                if d.option_count() == 0 {
                    return Err(format!(
                        "invariant 2: decision {:?} has no options: {:?}",
                        d.id, d.kind
                    ));
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

        // 8. The layer cache matches a fresh computation (a mutation that
        // changes characteristics without bumping the epoch would show here).
        if let Some(diff) = self.layer_cache_diff() {
            return Err(format!(
                "invariant 8: stale layer cache (missing epoch bump): {diff}"
            ));
        }

        // 6. Conservation: each in-game player owns exactly their deck's cards.
        for p in self.players.iter().filter(|p| p.in_game()) {
            let i = p.id.0 as usize;
            // Commanders aren't part of the deck list (CR 903.6).
            if let (Some(&n), Some(&have)) = (deck_sizes.get(i), owned.get(i))
                && have != n + self.commanders_owned(p.id)
            {
                return Err(format!(
                    "invariant 6: {} owns {have} objects, deck had {n}",
                    p.id
                ));
            }
        }

        // 9. Commanders (CR 903).
        self.commander_invariant()?;

        // 10. Restricted mana (CR 106.6) is part of the pool.
        for p in &self.players {
            for t in crate::card::ManaType::ALL {
                let restricted = self
                    .restricted
                    .iter()
                    .filter(|r| r.player == p.id && r.mana == t)
                    .count();
                if restricted > p.pool.get(t) as usize {
                    return Err(format!(
                        "invariant 10: {} has {restricted} restricted {t} but {} in the pool",
                        p.id,
                        p.pool.get(t)
                    ));
                }
            }
        }

        // Stack items refer to live objects on the stack, and so on.
        for s in self.stack.iter().filter(|s| s.is_spell()) {
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::card::CardPool;
    use crate::decks::test_deck;
    use crate::engine::{Game, GameRules, GameSetup};

    fn game() -> (Game, Vec<usize>) {
        let pool = Arc::new(CardPool::test_set());
        let decks: Vec<_> = (0..3).map(|i| test_deck(&pool, i, 40)).collect();
        let sizes = decks.iter().map(Vec::len).collect();
        let setup = GameSetup {
            rules: GameRules::default(),
            decks,
            seed: 4,
            first_player: None,
            commanders: Vec::new(),
        };
        (
            Game::new(pool, &setup).unwrap_or_else(|e| panic!("{e}")),
            sizes,
        )
    }

    #[test]
    fn clean_game_passes() {
        let (g, sizes) = game();
        assert_eq!(g.check_invariants(&sizes), Ok(()));
    }

    #[test]
    fn duplicate_zone_entry_is_caught() {
        let (mut g, sizes) = game();
        let id = g.players[0].library[0];
        g.players[0].hand.push(id);
        let err = g.check_invariants(&sizes).err().unwrap_or_default();
        assert!(err.contains("invariant 1"), "{err}");
    }

    #[test]
    fn dangling_object_is_caught() {
        let (mut g, sizes) = game();
        g.players[1].library.pop();
        let err = g.check_invariants(&sizes).err().unwrap_or_default();
        assert!(err.contains("dangling"), "{err}");
    }

    #[test]
    fn conservation_break_is_caught() {
        let (g, mut sizes) = game();
        sizes[2] += 1;
        let err = g.check_invariants(&sizes).err().unwrap_or_default();
        assert!(err.contains("invariant 6"), "{err}");
    }
}
