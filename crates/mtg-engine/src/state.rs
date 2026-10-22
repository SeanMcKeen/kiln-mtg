//! Game state: objects, zones, players, turn (`docs/03 Architecture/Game State Model.md`).
//!
//! Plain `Vec` arenas (M2 plan, D4). Objects hold a [`DefId`] into the shared
//! card pool and small `Copy` fields, so cloning the state is mostly memcpy.

use std::hash::{Hash, Hasher};

use mtg_core::{ObjectId, PlayerId};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::card::{DefId, ManaType};
use crate::decision::{CastCost, SymbolPay};

/// A zone (CR 400.1). Libraries, hands and graveyards belong to a player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ZoneId {
    Library(PlayerId),
    Hand(PlayerId),
    Graveyard(PlayerId),
    Battlefield,
    Stack,
    Exile,
    Command,
}

impl ZoneId {
    /// Hidden zones (CR 400.2): libraries and hands.
    pub fn is_hidden(self) -> bool {
        matches!(self, ZoneId::Library(_) | ZoneId::Hand(_))
    }
}

/// A game object (CR 109.1): a card in some zone, or a permanent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GameObject {
    pub def: DefId,
    pub owner: PlayerId,
    pub controller: PlayerId,
    pub zone: ZoneId,
    /// CR 110.5: status.
    pub tapped: bool,
    /// Damage marked on a permanent (CR 120.3e), removed in cleanup (CR 514.2).
    pub damage: u32,
    /// Dealt damage by a deathtouch source since SBAs were last checked (CR 704.5h).
    pub deathtouch_damage: bool,
    /// Turn number at which this object came under its controller's control (CR 302.6).
    pub control_since: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Slot {
    generation: u32,
    obj: Option<GameObject>,
}

/// Generational object arena. A freed slot is reused with a bumped generation,
/// so stale ids never alias new objects.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Arena {
    slots: Vec<Slot>,
    free: Vec<u32>,
}

impl Arena {
    pub fn alloc(&mut self, obj: GameObject) -> ObjectId {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.obj = Some(obj);
            ObjectId {
                index,
                generation: slot.generation,
            }
        } else {
            let index = u32::try_from(self.slots.len()).unwrap_or(u32::MAX);
            self.slots.push(Slot {
                generation: 0,
                obj: Some(obj),
            });
            ObjectId {
                index,
                generation: 0,
            }
        }
    }

    pub fn free(&mut self, id: ObjectId) -> Option<GameObject> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        let obj = slot.obj.take()?;
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(id.index);
        Some(obj)
    }

    pub fn get(&self, id: ObjectId) -> Option<&GameObject> {
        let slot = self.slots.get(id.index as usize)?;
        if slot.generation == id.generation {
            slot.obj.as_ref()
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut GameObject> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation == id.generation {
            slot.obj.as_mut()
        } else {
            None
        }
    }

    /// Every live object, in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (ObjectId, &GameObject)> {
        self.slots.iter().enumerate().filter_map(|(i, s)| {
            s.obj.as_ref().map(|o| {
                (
                    ObjectId {
                        index: u32::try_from(i).unwrap_or(u32::MAX),
                        generation: s.generation,
                    },
                    o,
                )
            })
        })
    }

    /// Number of slots (live or free); every live id's index is below this.
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub fn live_count(&self) -> usize {
        self.slots.len() - self.free.len()
    }
}

/// A player's mana pool (CR 106.4), indexed by [`ManaType`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ManaPool(pub [u32; 6]);

impl ManaPool {
    pub fn add(&mut self, t: ManaType, n: u32) {
        self.0[t.index()] += n;
    }

    pub fn get(&self, t: ManaType) -> u32 {
        self.0[t.index()]
    }

    pub fn total(&self) -> u32 {
        self.0.iter().sum()
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }

    pub fn clear(&mut self) {
        self.0 = [0; 6];
    }
}

/// Why a player lost (CR 104.3, 704.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LossReason {
    /// CR 704.5a
    ZeroLife,
    /// CR 704.5b
    DrewFromEmptyLibrary,
    /// CR 704.5c
    Poison,
    /// CR 104.3a
    Conceded,
    /// An effect says the player loses (CR 104.3e).
    Effect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlayerStatus {
    Playing,
    Lost(LossReason),
    Won,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Player {
    pub id: PlayerId,
    pub life: i64,
    pub poison: u32,
    pub pool: ManaPool,
    /// Top of library is the **last** element.
    pub library: Vec<ObjectId>,
    pub hand: Vec<ObjectId>,
    /// Top of graveyard is the last element.
    pub graveyard: Vec<ObjectId>,
    pub lands_played: u32,
    pub status: PlayerStatus,
    /// Attempted to draw from an empty library since SBAs were last checked (CR 704.5b).
    pub drew_from_empty: bool,
    pub mulligans: u32,
    pub kept: bool,
    /// Number of this player's most recent turn (0 = none yet). CR 302.6.
    pub last_turn: u32,
}

impl Player {
    pub fn in_game(&self) -> bool {
        self.status == PlayerStatus::Playing
    }
}

/// A target (CR 115.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Target {
    Object(ObjectId),
    Player(PlayerId),
}

/// A spell or activated ability on the stack (CR 112, 113.7, 405), with the
/// choices made while casting or activating it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StackItem {
    /// The spell, or the ability's source (which may since have left its zone).
    pub object: ObjectId,
    /// The spell's or source's definition (kept for last known information).
    pub source_def: DefId,
    /// `Some(i)`: activated ability `i` of the source; `None`: a spell.
    pub ability: Option<u8>,
    pub controller: PlayerId,
    pub targets: SmallVec<[Target; 1]>,
    /// The target slot of each entry in `targets` (CR 601.2c).
    pub slots: SmallVec<[u8; 1]>,
    /// Chosen modes, in printed order (CR 601.2b, 700.2).
    pub modes: SmallVec<[u8; 2]>,
    /// CR 601.2b, 118.9.
    pub cost: CastCost,
    /// The announced X (CR 107.3a).
    pub x: u32,
    /// How each hybrid or Phyrexian symbol of the cost is paid, in symbol order (CR 601.2b).
    pub pays: SmallVec<[SymbolPay; 1]>,
    /// For a triggered ability: what it saw when it triggered (CR 603.2).
    pub trigger: Option<crate::triggers::TriggerInfo>,
    /// For a delayed triggered ability: what to do and its creator's context (CR 603.7).
    pub(crate) delayed: Option<Box<crate::triggers::DelayedTrigger>>,
}

impl StackItem {
    pub fn new(
        object: ObjectId,
        source_def: DefId,
        ability: Option<u8>,
        controller: PlayerId,
    ) -> Self {
        Self {
            object,
            source_def,
            ability,
            controller,
            targets: SmallVec::new(),
            slots: SmallVec::new(),
            modes: SmallVec::new(),
            cost: CastCost::Normal,
            x: 0,
            pays: SmallVec::new(),
            trigger: None,
            delayed: None,
        }
    }

    pub fn is_spell(&self) -> bool {
        self.ability.is_none()
    }
}

/// Steps of a turn (CR 500.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Step {
    Untap,
    Upkeep,
    Draw,
    PrecombatMain,
    BeginCombat,
    DeclareAttackers,
    DeclareBlockers,
    FirstStrikeDamage,
    CombatDamage,
    EndCombat,
    PostcombatMain,
    End,
    Cleanup,
}

impl Step {
    pub fn is_main(self) -> bool {
        matches!(self, Step::PrecombatMain | Step::PostcombatMain)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TurnState {
    /// 0 before the first turn.
    pub number: u32,
    pub active: PlayerId,
    pub step: Step,
    /// Who has (or will next receive) priority. `None` = the active player at the next grant.
    pub priority: Option<PlayerId>,
    /// Consecutive passes (CR 117.4).
    pub passes: u32,
    /// Another cleanup step follows this one (CR 514.3a).
    pub extra_cleanup: bool,
}

/// An attacking creature (CR 508).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Attacker {
    pub object: ObjectId,
    pub defending: PlayerId,
    /// CR 509.1h: stays blocked even if its blockers leave combat.
    pub blocked: bool,
    pub blockers: SmallVec<[ObjectId; 2]>,
}

/// Combat bookkeeping for the current combat phase.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Combat {
    pub attackers: Vec<Attacker>,
    /// `(blocker, attacker)` pairs.
    pub blocks: Vec<(ObjectId, ObjectId)>,
    pub declared_attackers: bool,
    /// Creatures that had first or double strike as the first-strike damage step began (CR 702.4b).
    pub first_strikers: Vec<ObjectId>,
    pub had_first_strike_step: bool,
}

impl Combat {
    pub fn attacker(&self, id: ObjectId) -> Option<&Attacker> {
        self.attackers.iter().find(|a| a.object == id)
    }

    pub fn is_blocking(&self, id: ObjectId) -> bool {
        self.blocks.iter().any(|(b, _)| *b == id)
    }

    /// Removes a permanent from combat (CR 506.4).
    pub fn remove(&mut self, id: ObjectId) {
        self.attackers.retain(|a| a.object != id);
        for a in &mut self.attackers {
            a.blockers.retain(|b| *b != id);
        }
        self.blocks.retain(|(b, a)| *b != id && *a != id);
    }
}

/// FNV-1a 64-bit, little-endian for every integer width, so state hashes
/// are identical across platforms and Rust versions (M2 plan, D9).
#[derive(Debug, Clone, Copy)]
pub struct StableHasher(u64);

impl Default for StableHasher {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn write_u8(&mut self, i: u8) {
        self.write(&[i]);
    }
    fn write_u16(&mut self, i: u16) {
        self.write(&i.to_le_bytes());
    }
    fn write_u32(&mut self, i: u32) {
        self.write(&i.to_le_bytes());
    }
    fn write_u64(&mut self, i: u64) {
        self.write(&i.to_le_bytes());
    }
    fn write_usize(&mut self, i: usize) {
        self.write(&(i as u64).to_le_bytes());
    }
    fn write_i32(&mut self, i: i32) {
        self.write(&i.to_le_bytes());
    }
    fn write_i64(&mut self, i: i64) {
        self.write(&i.to_le_bytes());
    }
}

pub fn stable_hash<T: Hash + ?Sized>(value: &T) -> u64 {
    let mut h = StableHasher::default();
    value.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj() -> GameObject {
        GameObject {
            def: DefId(0),
            owner: PlayerId(0),
            controller: PlayerId(0),
            zone: ZoneId::Battlefield,
            tapped: false,
            damage: 0,
            deathtouch_damage: false,
            control_since: 0,
        }
    }

    #[test]
    fn arena_generations() {
        let mut a = Arena::default();
        let x = a.alloc(obj());
        assert!(a.free(x).is_some());
        let y = a.alloc(obj());
        assert_eq!(x.index, y.index);
        assert_ne!(x, y);
        assert!(a.get(x).is_none());
        assert!(a.get(y).is_some());
        assert!(a.free(x).is_none());
        assert_eq!(a.live_count(), 1);
    }

    #[test]
    fn stable_hash_is_fixed() {
        // Pinned value: changing the hasher breaks golden replays.
        assert_eq!(stable_hash(&42u64), stable_hash(&42u64));
        assert_eq!(stable_hash(&0u8), 0xaf63_bd4c_8601_b7df);
    }
}
