//! `mtg-engine`: game state, rules procedures, decisions, events, views, replay.
//!
//! The engine is a decision state machine (ADR-0004): [`Game::pending`]
//! exposes the one decision a player must make, with every legal option
//! enumerated, and [`Game::apply`] answers it. Nothing else mutates a game.
//! See `docs/09 Roadmap/Milestones/M2 Core Game Loop.md` for the M2 scope.

pub mod builder;
pub mod card;
mod cast;
mod combat;
pub mod decision;
pub mod decks;
mod engine;
pub mod events;
mod invariants;
pub mod mana;
pub mod replay;
pub mod rng;
mod sba;
pub mod state;
pub mod view;

pub use builder::{GameBuilder, PermanentOpts};
pub use card::{CardDef, CardPool, DefId, Effect, Keywords, ManaType, TargetSpec};
pub use decision::{
    Action, Choice, ChoiceError, DamageRecipient, DecisionId, DecisionKind, ManaStep, PaymentPlan,
    PendingDecision,
};
pub use engine::{AbortReason, Game, GameResult, GameRules, GameSetup, SetupError};
pub use events::Event;
pub use replay::{Divergence, Recorder, Replay};
pub use state::{GameObject, LossReason, Player, PlayerStatus, StackItem, Step, Target, ZoneId};
pub use view::{PlayerView, PublicPlayer, ViewSnapshot, VisibleObject};
