//! The Commander variant (CR 903; `docs/03 Architecture/Subsystems/Commander.md`,
//! M6 plan C1–C6).
//!
//! - Being a commander is a property of the card, not a characteristic
//!   (CR 903.3): the table follows the card from zone to zone, and tokens,
//!   copies and copy effects never become commanders.
//! - Returning a commander to the command zone is always its owner's choice:
//!   the CR 903.9a state-based action and the CR 903.9b replacement are
//!   `CommanderZone` decisions. A 903.9b answer is asked before the move and
//!   recorded; the move consumes it.

use mtg_core::{Colors, ObjectId, PlayerId};
use serde::{Deserialize, Serialize};

use crate::card::DefId;
use crate::decision::DecisionKind;
use crate::engine::Game;
use crate::state::ZoneId;

/// One commander (CR 903.3).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Commander {
    pub owner: PlayerId,
    pub def: DefId,
    /// The card now; `None` once it has left the game.
    pub object: Option<ObjectId>,
    /// Times it has been cast from the command zone (CR 903.8).
    pub casts: u32,
    /// Combat damage it has dealt to each player, by seat (CR 903.10a).
    pub damage: Vec<u32>,
}

/// Commander state of a game. Empty outside Commander games.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CommanderState {
    pub(crate) commanders: Vec<Commander>,
    /// The command zone (CR 408).
    pub(crate) zone: Vec<ObjectId>,
    /// Owners' answers to "put it into the command zone instead?" (CR 903.9b),
    /// recorded before the move that consumes them.
    pub(crate) answers: Vec<(ObjectId, bool)>,
    /// Commanders put into a graveyard or exile since state-based actions
    /// were last checked (CR 903.9a).
    pub(crate) since_sba: Vec<ObjectId>,
    /// This check's CR 903.9a offers still to be answered, and those accepted.
    pub(crate) offers: Vec<ObjectId>,
    pub(crate) accepted: Vec<ObjectId>,
    /// Spells being cast from the command zone and the tax they pay (CR 903.8).
    pub(crate) taxed: Vec<(ObjectId, u32)>,
    /// Commander moves into a hand or library made without the owner's
    /// CR 903.9b answer: an engine bug that invariant 9 reports (not hashed).
    pub(crate) unasked: u32,
}

impl std::hash::Hash for CommanderState {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.commanders.hash(h);
        self.zone.hash(h);
        self.answers.hash(h);
        self.since_sba.hash(h);
        self.offers.hash(h);
        self.accepted.hash(h);
        self.taxed.hash(h);
    }
}

impl CommanderState {
    pub(crate) fn is_empty(&self) -> bool {
        self.commanders.is_empty()
    }
}

/// Combat damage from one commander that makes a player lose (CR 704.6c).
pub const COMMANDER_DAMAGE_LIMIT: u32 = 21;

impl Game {
    /// The commanders of this game, in setup order.
    pub fn commanders(&self) -> &[Commander] {
        &self.cmd.commanders
    }

    /// The command zone's objects (CR 408).
    pub fn command_zone(&self) -> &[ObjectId] {
        &self.cmd.zone
    }

    /// The commander table index of object `id`, if it is a commander.
    pub(crate) fn commander_index(&self, id: ObjectId) -> Option<usize> {
        if self.cmd.is_empty() {
            return None;
        }
        self.cmd
            .commanders
            .iter()
            .position(|c| c.object == Some(id))
    }

    /// Whether object `id` is a commander (CR 903.3).
    pub fn is_commander(&self, id: ObjectId) -> bool {
        self.commander_index(id).is_some()
    }

    /// CR 903.3d: `p` controls a permanent that is a commander (anyone's).
    pub(crate) fn controls_commander(&self, p: PlayerId) -> bool {
        self.cmd.commanders.iter().any(|c| {
            c.object
                .and_then(|id| self.obj(id))
                .is_some_and(|o| o.zone == ZoneId::Battlefield && o.controller == p)
        })
    }

    /// CR 903.4, 702.124c: `p`'s commanders' combined color identity;
    /// `None` if `p` has no commander (CR 903.4f: undefined).
    pub(crate) fn commander_identity(&self, p: PlayerId) -> Option<Colors> {
        let mut out: Option<Colors> = None;
        for c in self.cmd.commanders.iter().filter(|c| c.owner == p) {
            let id = self.def(c.def).color_identity;
            out = Some(out.map_or(id, |o| o.union(id)));
        }
        out
    }

    /// CR 903.8: the tax for casting `id` now (it's in the command zone) or
    /// being paid by spell `id` (cast from there).
    pub(crate) fn commander_tax(&self, id: ObjectId) -> u32 {
        if self.cmd.is_empty() {
            return 0;
        }
        if let Some((_, tax)) = self.cmd.taxed.iter().find(|(s, _)| *s == id) {
            return *tax;
        }
        match (self.obj(id), self.commander_index(id)) {
            (Some(o), Some(i)) if o.zone == ZoneId::Command => 2 * self.cmd.commanders[i].casts,
            _ => 0,
        }
    }

    /// The tax for casting `card` if it's a commander in the command zone
    /// (read before it moves to the stack).
    pub(crate) fn command_zone_tax(&self, card: ObjectId) -> Option<u32> {
        let o = self.obj(card)?;
        (o.zone == ZoneId::Command && self.is_commander(card)).then(|| self.commander_tax(card))
    }

    /// CR 903.8: a commander cast from the command zone is now `spell`; the
    /// tax is fixed and the cast counted.
    pub(crate) fn commander_cast(&mut self, spell: ObjectId, tax: u32) {
        if let Some(i) = self.commander_index(spell) {
            self.cmd.commanders[i].casts += 1;
            self.cmd.taxed.push((spell, tax));
        }
    }

    /// Keeps the table pointing at the card as it changes zones (CR 400.7,
    /// 903.3). Called by the zone-change primitive.
    pub(crate) fn commander_moved(&mut self, old: ObjectId, new: Option<ObjectId>, to: ZoneId) {
        if self.cmd.is_empty() {
            return;
        }
        self.cmd.taxed.retain(|(s, _)| *s != old);
        let Some(i) = self.commander_index(old) else {
            return;
        };
        self.cmd.commanders[i].object = new;
        if let Some(new) = new
            && matches!(to, ZoneId::Graveyard(_) | ZoneId::Exile)
        {
            self.cmd.since_sba.push(new);
        }
    }

    /// CR 903.9b: whether moving `id` to `to` needs its owner's answer first:
    /// it's a commander, `to` is its owner's hand or library, and no answer
    /// has been recorded. Returns the owner who decides.
    pub(crate) fn commander_choice_needed(&self, id: ObjectId, to: ZoneId) -> Option<PlayerId> {
        if self.cmd.is_empty() {
            return None;
        }
        let i = self.commander_index(id)?;
        let owner = self.cmd.commanders[i].owner;
        // Moving within a zone isn't a zone change.
        if self.obj(id)?.zone == to {
            return None;
        }
        let into_hand_or_library = matches!(to, ZoneId::Hand(p) | ZoneId::Library(p) if p == owner);
        (into_hand_or_library && !self.cmd.answers.iter().any(|(x, _)| *x == id)).then_some(owner)
    }

    /// The first of `ids` whose move to `to` needs a CR 903.9b answer, as
    /// the decision to ask.
    pub(crate) fn commander_question(
        &self,
        ids: impl IntoIterator<Item = ObjectId>,
        to: impl Fn(ObjectId) -> Option<ZoneId>,
    ) -> Option<(PlayerId, DecisionKind)> {
        if self.cmd.is_empty() {
            return None;
        }
        ids.into_iter().find_map(|id| {
            let dest = to(id)?;
            self.commander_choice_needed(id, dest).map(|owner| {
                (
                    owner,
                    DecisionKind::CommanderZone {
                        commander: id,
                        to: dest,
                    },
                )
            })
        })
    }

    /// CR 903.9b for `p`'s next draw: their top card is a commander.
    pub(crate) fn draw_question(&self, p: PlayerId) -> Option<(PlayerId, DecisionKind)> {
        if self.cmd.is_empty() {
            return None;
        }
        let top = *self.player(p).library.last()?;
        self.commander_question([top], |_| Some(ZoneId::Hand(p)))
    }

    /// Records the owner's CR 903.9b answer for the coming move of `id`.
    pub(crate) fn commander_answer(&mut self, id: ObjectId, command_zone: bool) {
        self.cmd.answers.push((id, command_zone));
    }

    /// Where a move of `id` to `to` actually goes: CR 903.9b may redirect a
    /// commander to the command zone. Consumes the recorded answer.
    pub(crate) fn commander_destination(&mut self, id: ObjectId, to: ZoneId) -> ZoneId {
        if self.cmd.is_empty() || !matches!(to, ZoneId::Hand(_) | ZoneId::Library(_)) {
            return to;
        }
        let Some(i) = self.commander_index(id) else {
            return to;
        };
        if !matches!(to, ZoneId::Hand(p) | ZoneId::Library(p) if p == self.cmd.commanders[i].owner)
        {
            return to;
        }
        let Some(pos) = self.cmd.answers.iter().position(|(x, _)| *x == id) else {
            self.cmd.unasked += 1;
            return to;
        };
        let (_, yes) = self.cmd.answers.remove(pos);
        if yes { ZoneId::Command } else { to }
    }

    /// Clears answers no move consumed (the move didn't happen).
    pub(crate) fn clear_commander_answers(&mut self) {
        if !self.cmd.answers.is_empty() {
            self.cmd.answers.clear();
        }
    }

    // ---- CR 903.9a (a state-based action) ------------------------------------

    /// Starts a state-based action check: the commanders put into a graveyard
    /// or exile since the last check that are still there get an offer.
    pub(crate) fn begin_commander_offers(&mut self) {
        if self.cmd.is_empty() {
            return;
        }
        let since = std::mem::take(&mut self.cmd.since_sba);
        self.cmd.offers = since
            .into_iter()
            .filter(|id| {
                self.obj(*id)
                    .is_some_and(|o| matches!(o.zone, ZoneId::Graveyard(_) | ZoneId::Exile))
            })
            .collect();
    }

    /// Whether a CR 903.9a offer would be made at the next check.
    pub(crate) fn commander_offer_pending(&self) -> bool {
        self.cmd.since_sba.iter().any(|id| {
            self.obj(*id)
                .is_some_and(|o| matches!(o.zone, ZoneId::Graveyard(_) | ZoneId::Exile))
        })
    }

    /// The next CR 903.9a offer to ask.
    pub(crate) fn next_commander_offer(&self) -> Option<(PlayerId, DecisionKind)> {
        let id = *self.cmd.offers.first()?;
        let o = self.obj(id)?;
        Some((
            o.owner,
            DecisionKind::CommanderZone {
                commander: id,
                to: o.zone,
            },
        ))
    }

    pub(crate) fn commander_offer_answer(&mut self, yes: bool) {
        if self.cmd.offers.is_empty() {
            return;
        }
        let id = self.cmd.offers.remove(0);
        if yes {
            self.cmd.accepted.push(id);
        }
    }

    /// The accepted CR 903.9a moves, performed with the check's other actions.
    pub(crate) fn take_accepted_offers(&mut self) -> Vec<ObjectId> {
        std::mem::take(&mut self.cmd.accepted)
    }

    // ---- commander damage (CR 903.10a, 704.6c) --------------------------------

    pub(crate) fn record_commander_damage(&mut self, source: ObjectId, p: PlayerId, amount: u32) {
        if let Some(i) = self.commander_index(source) {
            let n = self.players.len();
            let c = &mut self.cmd.commanders[i];
            if c.damage.len() < n {
                c.damage.resize(n, 0);
            }
            c.damage[p.0 as usize] += amount;
        }
    }

    /// CR 704.6c: players dealt 21 or more combat damage by one commander.
    pub(crate) fn commander_damage_losers(&self) -> Vec<PlayerId> {
        let mut out: Vec<PlayerId> = Vec::new();
        for c in &self.cmd.commanders {
            for (seat, dealt) in c.damage.iter().enumerate() {
                let p = PlayerId(u8::try_from(seat).unwrap_or(0));
                if *dealt >= COMMANDER_DAMAGE_LIMIT && !out.contains(&p) {
                    out.push(p);
                }
            }
        }
        out
    }

    /// Invariant 9: the commander table refers to live cards of the right
    /// owner and definition, and every CR 903.9b move was asked.
    pub(crate) fn commander_invariant(&self) -> Result<(), String> {
        if self.cmd.unasked > 0 {
            return Err(format!(
                "invariant 9: {} commander move(s) into a hand or library without the owner's CR 903.9b choice",
                self.cmd.unasked
            ));
        }
        for c in &self.cmd.commanders {
            let Some(id) = c.object else { continue };
            match self.obj(id) {
                Some(o) if o.owner == c.owner && o.def == c.def => {}
                Some(o) => {
                    return Err(format!(
                        "invariant 9: commander {id} is {:?} owned by {}, table says {:?} owned by {}",
                        o.def, o.owner, c.def, c.owner
                    ));
                }
                None if self.in_game(c.owner) => {
                    return Err(format!("invariant 9: commander {id} isn't a live object"));
                }
                None => {}
            }
        }
        Ok(())
    }

    /// Commanders `p` owns that are still in the game.
    pub(crate) fn commanders_owned(&self, p: PlayerId) -> usize {
        self.cmd
            .commanders
            .iter()
            .filter(|c| c.owner == p && c.object.is_some())
            .count()
    }

    /// Combat damage commander `index` has dealt to `p` (CR 903.10a).
    pub fn commander_damage(&self, index: usize, p: PlayerId) -> u32 {
        self.cmd
            .commanders
            .get(index)
            .and_then(|c| c.damage.get(p.0 as usize))
            .copied()
            .unwrap_or(0)
    }
}
