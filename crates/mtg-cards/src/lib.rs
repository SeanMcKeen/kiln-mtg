//! `mtg-cards`: how cards get behaviour.
//!
//! The behaviour IR and its front-ends are decided by ADR-0007 in M3. M1 ships
//! only the coverage model from `docs/06 Card Data/Card Data Model.md`, so that
//! every card has an explicit, honest support status from day one (Design
//! Principle 9: fail loudly on unsupported cards).

pub mod coverage;

pub use coverage::{CardDefinition, Coverage, CoverageLevel, Provenance, UnsupportedSpan};
