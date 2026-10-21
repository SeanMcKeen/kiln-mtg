//! `mtg-core`: primitive types shared by every other crate.
//!
//! Nothing here knows about game procedures or specific cards. Every type cites
//! the Comprehensive Rules section it models. See `docs/03 Architecture/Crate Layout.md`.

pub mod color;
pub mod cr;
pub mod ids;
pub mod mana;
pub mod stat;
pub mod types;

pub use color::{Color, Colors};
pub use cr::CrRef;
pub use ids::{ObjectId, OracleId, PlayerId};
pub use mana::{ManaCost, ManaSymbol};
pub use stat::PrintedStat;
pub use types::{CardType, Subtype, Supertype, TypeLine};
