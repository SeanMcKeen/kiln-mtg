//! Experiments (M7): a `kiln-spec/1` spec resolved into decks, played by the
//! batch runner, and summarised into a `kiln-report/1` report.
//! Design: `docs/09 Roadmap/Milestones/M7 Simulation Lab v1.md` (plan S1–S14).

pub mod decks;
pub mod facts;
pub mod play;
pub mod report;
pub mod stats;
pub mod store;

pub use decks::{ResolvedDeck, resolve_decks};
pub use play::{Config, Experiment, OPPONENT, Played};
pub use report::write_report;
pub use store::{Progress, RunOptions, read_manifest, read_records, run};

/// Why an experiment can't run. [`ExperimentError::exit_code`] follows
/// `docs/07 Interfaces/CLI & REPL.md`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExperimentError {
    #[error(transparent)]
    Spec(#[from] kiln_protocol::SpecError),
    #[error("deck '{id}': {message}")]
    Deck { id: String, message: String },
    #[error("deck '{id}': {gate}: {}", cards.join(", "))]
    Coverage {
        id: String,
        cards: Vec<String>,
        gate: String,
    },
    #[error("run failed: {0}")]
    Run(String),
    #[error("determinism divergence: {0}")]
    Divergence(String),
}

impl ExperimentError {
    /// 2 invalid input, 3 coverage gate, 4 run failed, 5 determinism divergence.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Spec(_) | Self::Deck { .. } => 2,
            Self::Coverage { .. } => 3,
            Self::Run(_) => 4,
            Self::Divergence(_) => 5,
        }
    }
}
