//! Occurred events (`docs/03 Architecture/Event System.md`, M2 plan D11).
//!
//! An append-only log; each event names the rule behind it. Proposed events
//! and replacement effects arrive in M4.

use mtg_cards::ir::CounterKind;
use mtg_core::{CrRef, ObjectId, PlayerId};
use serde::{Deserialize, Serialize};

use crate::card::{DefId, ManaType};
use crate::state::{LossReason, Step, Target, ZoneId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    GameStarted {
        starting_player: PlayerId,
    },
    Mulligan {
        player: PlayerId,
        count: u32,
    },
    Kept {
        player: PlayerId,
        hand_size: u32,
    },
    TurnBegan {
        number: u32,
        active: PlayerId,
    },
    StepBegan {
        step: Step,
    },
    /// A card was drawn (CR 121.1). `object` is the new hand object.
    Drew {
        player: PlayerId,
        object: ObjectId,
    },
    /// A zone change (CR 400.7: `new` is a new object).
    ZoneChange {
        def: DefId,
        owner: PlayerId,
        from: ZoneId,
        to: ZoneId,
        old: ObjectId,
        new: ObjectId,
    },
    LandPlayed {
        player: PlayerId,
        object: ObjectId,
    },
    SpellCast {
        player: PlayerId,
        object: ObjectId,
        def: DefId,
    },
    ManaAdded {
        player: PlayerId,
        mana: ManaType,
        source: ObjectId,
    },
    /// An activated ability became activated (CR 602.2b, applying 601.2i).
    AbilityActivated {
        player: PlayerId,
        source: ObjectId,
        def: DefId,
        ability: u8,
    },
    Resolved {
        object: ObjectId,
        def: DefId,
    },
    /// All targets illegal on resolution (CR 608.2b).
    DidNotResolve {
        object: ObjectId,
        def: DefId,
    },
    Damage {
        source: ObjectId,
        target: Target,
        amount: u32,
        combat: bool,
    },
    LifeGained {
        player: PlayerId,
        amount: u32,
    },
    LifeLost {
        player: PlayerId,
        amount: u32,
    },
    Tapped {
        object: ObjectId,
    },
    Untapped {
        object: ObjectId,
    },
    AttackerDeclared {
        object: ObjectId,
        defending: PlayerId,
    },
    BlockerDeclared {
        blocker: ObjectId,
        attacker: ObjectId,
    },
    Destroyed {
        object: ObjectId,
    },
    /// Cards shown to all players (CR 701.20a). Ids are the objects as revealed.
    Revealed {
        player: PlayerId,
        objects: Vec<ObjectId>,
    },
    /// A library was shuffled (CR 701.24a).
    Shuffled {
        player: PlayerId,
    },
    /// A player searched their library (CR 701.23a). What they found is private unless revealed.
    Searched {
        player: PlayerId,
        found: u32,
    },
    /// A player scried (CR 701.22a).
    Scried {
        player: PlayerId,
        count: u32,
    },
    /// `object` is the graveyard object (CR 701.9a).
    Discarded {
        player: PlayerId,
        object: ObjectId,
    },
    /// `object` is the permanent that was sacrificed (CR 701.21a).
    Sacrificed {
        object: ObjectId,
    },
    /// `object` is the spell that was countered (CR 701.6a).
    Countered {
        object: ObjectId,
    },
    /// A public choice made while an effect resolved (CR 608.2d), such as a card name.
    Chose {
        player: PlayerId,
        choice: String,
    },
    /// Counters were put on an object (CR 122.6).
    CountersPut {
        object: ObjectId,
        counter: CounterKind,
        amount: u32,
    },
    CountersRemoved {
        object: ObjectId,
        counter: CounterKind,
        amount: u32,
    },
    /// A token was created (CR 111.2).
    TokenCreated {
        player: PlayerId,
        object: ObjectId,
        def: DefId,
    },
    /// An effect says this player wins the game (CR 104.2b).
    WonByEffect {
        player: PlayerId,
    },
    PlayerLost {
        player: PlayerId,
        reason: LossReason,
    },
    /// The player left the game (CR 800.4a).
    PlayerLeft {
        player: PlayerId,
    },
    GameOver,
}

impl Event {
    /// The rule that produces this event.
    pub fn cr(&self) -> CrRef {
        let s = match self {
            Event::GameStarted { .. } => "103.1",
            Event::Mulligan { .. } | Event::Kept { .. } => "103.5",
            Event::TurnBegan { .. } => "500.1",
            Event::StepBegan { .. } => "500.1",
            Event::Drew { .. } => "121.1",
            Event::ZoneChange { .. } => "400.7",
            Event::LandPlayed { .. } => "305.1",
            Event::SpellCast { .. } => "601.2i",
            Event::ManaAdded { .. } => "605.1a",
            Event::AbilityActivated { .. } => "602.2b",
            Event::Resolved { .. } => "608.2",
            Event::DidNotResolve { .. } => "608.2b",
            Event::Damage { combat: true, .. } => "510.2",
            Event::Damage { .. } => "120.1",
            Event::LifeGained { .. } | Event::LifeLost { .. } => "119.3",
            Event::Tapped { .. } | Event::Untapped { .. } => "110.5",
            Event::AttackerDeclared { .. } => "508.1",
            Event::BlockerDeclared { .. } => "509.1",
            Event::Destroyed { .. } => "701.8a",
            Event::Revealed { .. } => "701.20a",
            Event::Shuffled { .. } => "701.24a",
            Event::Searched { .. } => "701.23a",
            Event::Scried { .. } => "701.22a",
            Event::Discarded { .. } => "701.9a",
            Event::Sacrificed { .. } => "701.21a",
            Event::Countered { .. } => "701.6a",
            Event::Chose { .. } => "608.2d",
            Event::WonByEffect { .. } => "104.2b",
            Event::CountersPut { .. } | Event::CountersRemoved { .. } => "122.1",
            Event::TokenCreated { .. } => "111.2",
            Event::PlayerLost { .. } => "104.3",
            Event::PlayerLeft { .. } => "800.4a",
            Event::GameOver => "104.1",
        };
        s.parse()
            .unwrap_or_else(|_| CrRef::new(100, None, None).unwrap_or_else(|| unreachable!()))
    }
}
