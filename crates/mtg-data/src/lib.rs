//! `mtg-data`: card data ingestion.
//!
//! Scryfall bulk `oracle_cards` (ADR-0009, proposed) → normalized
//! [`CardRecord`]s → a versioned binary [`CardDb`]. Card data is fetched at
//! build time and cached under `data/cache/`; it is never committed
//! (ADR-0002). See `docs/06 Card Data/Ingestion Pipeline.md`.

pub mod build;
pub mod db;
#[cfg(feature = "fetch")]
pub mod fetch;
pub mod names;
pub mod normalize;
pub mod record;
pub mod scryfall;

pub use build::{BuildInput, BuildReport, build};
pub use db::{CardDb, CardDbVersion, DbError};
pub use names::NameMatch;
pub use record::{CardRecord, FaceRecord, Layout, Legality};

/// Default cache directory, relative to the repository root.
pub const DEFAULT_CACHE_DIR: &str = "data/cache";
