//! Deck import and validation (`docs/13 Simulation/Deck Import.md`).
//!
//! [`parse`] turns a text or CSV export into a [`ParsedDeck`] without touching
//! the card DB. [`validate`] resolves names against a [`kiln_data::CardDb`] and
//! checks the format's deck-construction rules, producing a [`DeckReport`].

pub mod parse;
pub mod validate;

pub use parse::{DeckEntry, ParsedDeck, Section, parse_csv, parse_deck, parse_text};
pub use validate::{
    CardLine, CoverageSummary, DeckRecord, DeckReport, Issue, ValidateOptions, validate,
    validate_with,
};
