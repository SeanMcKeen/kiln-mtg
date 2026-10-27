//! `kiln-cards`: how cards get behaviour.
//!
//! - [`coverage`]: the coverage model from `docs/06 Card Data/Card Data Model.md`
//!   (every card has an honest support status; Design Principle 9).
//! - [`ir`]: the card IR v1 draft (M3 spike, ADR-0007).
//! - [`defs`]: YAML card definition files, the DSL candidate.

pub mod coverage;
pub mod defs;
// The IR is built once per card definition, not in the game loop, so enum
// variant size doesn't matter yet; revisit (boxing) when the engine
// interprets it in Phase B.
#[allow(clippy::large_enum_variant)]
pub mod ir;
#[allow(clippy::large_enum_variant)]
pub mod parse;
pub mod render;

pub use coverage::{CardDefinition, Coverage, CoverageLevel, Provenance, UnsupportedSpan};
