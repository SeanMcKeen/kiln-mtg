//! Counters (CR 122) and tokens (CR 111)
//! (`docs/03 Architecture/Subsystems/Counters.md`, M3 plan B-D9, B-D10).
//!
//! Counters live in a side table keyed by object id. A zone change makes a
//! new object (CR 400.7), so counters never follow it (CR 122.2): the old
//! id's entries are dropped when the object leaves.

use kiln_cards::ir::CounterKind;
use kiln_types::{ObjectId, PlayerId};

use crate::card::DefId;
use crate::engine::Game;
use crate::events::Event;
use crate::state::{GameObject, ZoneId};

impl Game {
    /// Counters of `kind` on object `id`.
    pub fn counters_on(&self, id: ObjectId, kind: CounterKind) -> u32 {
        self.counters
            .iter()
            .find(|(o, k, _)| *o == id && *k == kind)
            .map_or(0, |(_, _, n)| *n)
    }

    /// Every kind of counter on object `id`, in kind order.
    pub fn counters_of(&self, id: ObjectId) -> Vec<(CounterKind, u32)> {
        self.counters
            .iter()
            .filter(|(o, _, n)| *o == id && *n > 0)
            .map(|(_, k, n)| (*k, *n))
            .collect()
    }

    /// CR 122.6: put counters on a permanent.
    pub(crate) fn add_counters(&mut self, id: ObjectId, kind: CounterKind, n: u32) {
        self.bump_layers();
        if n == 0 || !self.obj(id).is_some_and(|o| o.zone == ZoneId::Battlefield) {
            return;
        }
        match self
            .counters
            .iter_mut()
            .find(|(o, k, _)| *o == id && *k == kind)
        {
            Some((_, _, have)) => *have += n,
            None => {
                self.counters.push((id, kind, n));
                // Sorted, so the table (and the state hash) doesn't depend on history.
                self.counters.sort_unstable();
            }
        }
        self.log(Event::CountersPut {
            object: id,
            counter: kind,
            amount: n,
        });
    }

    /// Removes up to `n` counters; returns how many were removed (CR 609.3).
    pub(crate) fn remove_counters(&mut self, id: ObjectId, kind: CounterKind, n: u32) -> u32 {
        self.bump_layers();
        let Some(pos) = self
            .counters
            .iter()
            .position(|(o, k, _)| *o == id && *k == kind)
        else {
            return 0;
        };
        let removed = self.counters[pos].2.min(n);
        self.counters[pos].2 -= removed;
        if self.counters[pos].2 == 0 {
            self.counters.remove(pos);
        }
        if removed > 0 {
            self.log(Event::CountersRemoved {
                object: id,
                counter: kind,
                amount: removed,
            });
        }
        removed
    }

    /// CR 122.2: counters cease to exist when their object leaves its zone.
    pub(crate) fn drop_counters(&mut self, id: ObjectId) {
        if !self.counters.is_empty() {
            self.counters.retain(|(o, _, _)| *o != id);
        }
    }

    /// Net power/toughness change from +1/+1 and -1/-1 counters (layer 7c, CR 613.4c).
    pub(crate) fn counter_pt(&self, id: ObjectId) -> i32 {
        if self.counters.is_empty() {
            return 0;
        }
        let plus = i64::from(self.counters_on(id, CounterKind::PlusOne));
        let minus = i64::from(self.counters_on(id, CounterKind::MinusOne));
        i32::try_from(plus - minus).unwrap_or(0)
    }

    /// CR 111.2: creates `n` tokens owned and controlled by `owner`, on the battlefield.
    pub(crate) fn create_tokens(
        &mut self,
        def: DefId,
        owner: PlayerId,
        n: u32,
    ) -> smallvec::SmallVec<[ObjectId; 2]> {
        self.bump_layers();
        let mut made = smallvec::SmallVec::new();
        for _ in 0..n {
            let timestamp = self.next_timestamp();
            let id = self.objects.alloc(GameObject {
                def,
                owner,
                controller: owner,
                zone: ZoneId::Battlefield,
                tapped: false,
                damage: 0,
                deathtouch_damage: false,
                control_since: self.turn.number,
                timestamp,
            });
            self.battlefield.push(id);
            made.push(id);
            self.log(Event::TokenCreated {
                player: owner,
                object: id,
                def,
            });
        }
        made
    }

    /// CR 704.5d: tokens in a zone other than the battlefield.
    /// Also copies of cards outside the stack and battlefield (CR 704.5e).
    pub(crate) fn tokens_off_battlefield(&self) -> Vec<ObjectId> {
        let mut out: Vec<ObjectId> = self
            .tokens_off_field
            .iter()
            .copied()
            .filter(|id| self.obj(*id).is_some_and(|o| o.zone != ZoneId::Battlefield))
            .collect();
        out.extend(self.copies.iter().copied().filter(|id| {
            self.obj(*id)
                .is_some_and(|o| !matches!(o.zone, ZoneId::Battlefield | ZoneId::Stack))
        }));
        out
    }

    /// CR 704.5d: the token ceases to exist.
    pub(crate) fn cease_to_exist(&mut self, id: ObjectId) {
        let Some(zone) = self.obj(id).map(|o| o.zone) else {
            return;
        };
        self.detach_object(id, zone);
        self.commander_moved(id, None, zone);
        self.objects.free(id);
        self.drop_counters(id);
        self.bump_layers();
        self.tokens_off_field.retain(|x| *x != id);
        if !self.copies.is_empty() {
            self.copies.retain(|x| *x != id);
        }
    }

    /// CR 704.5q: permanents with both +1/+1 and -1/-1 counters, and how many pairs to remove.
    pub(crate) fn counter_pairs(&self) -> Vec<(ObjectId, u32)> {
        if self.counters.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<(ObjectId, u32)> = Vec::new();
        for (id, _, _) in &self.counters {
            if out.last().is_some_and(|(o, _)| o == id) {
                continue;
            }
            let n = self
                .counters_on(*id, CounterKind::PlusOne)
                .min(self.counters_on(*id, CounterKind::MinusOne));
            if n > 0 {
                out.push((*id, n));
            }
        }
        out
    }
}
