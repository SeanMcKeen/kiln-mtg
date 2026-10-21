//! Replays (`docs/03 Architecture/Determinism & Replay.md`, ADR-0005).
//!
//! A replay is the setup plus the choice log `(decision_id, option)`.
//! Replaying re-derives every decision; any mismatch is a determinism bug and
//! is reported at the first divergence.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::card::CardPool;
use crate::decision::{Choice, DecisionId};
use crate::engine::{Game, GameResult, GameSetup};

pub const REPLAY_FORMAT: &str = "mtg-replay/1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replay {
    pub format: String,
    pub engine: String,
    /// Stable hash of the card pool, so a replay can't silently run against different cards.
    pub pool_hash: u64,
    pub setup: GameSetup,
    pub choices: Vec<(u64, u32)>,
    pub final_hash: u64,
    pub result: Option<GameResult>,
}

/// Where a replay stopped matching.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Divergence {
    #[error("setup failed: {0}")]
    Setup(String),
    #[error("card pool differs (replay {expected:#x}, current {actual:#x})")]
    Pool { expected: u64, actual: u64 },
    #[error("choice {index}: expected decision {expected}, engine offered {actual:?}")]
    Decision {
        index: usize,
        expected: u64,
        actual: Option<u64>,
    },
    #[error("choice {index}: option {option} rejected: {error}")]
    Rejected {
        index: usize,
        option: u32,
        error: String,
    },
    #[error("final state hash {actual:#x} differs from recorded {expected:#x}")]
    FinalHash { expected: u64, actual: u64 },
}

/// Stable hash of a card pool's definitions.
pub fn pool_hash(pool: &CardPool) -> u64 {
    crate::state::stable_hash(&serde_json::to_string(pool).unwrap_or_default())
}

/// Records choices while a game is played.
#[derive(Debug, Clone)]
pub struct Recorder {
    choices: Vec<(u64, u32)>,
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            choices: Vec::new(),
        }
    }

    /// Applies `option` to the pending decision and records it.
    pub fn apply(
        &mut self,
        game: &mut Game,
        option: u32,
    ) -> Result<(), crate::decision::ChoiceError> {
        let Some(d) = game.pending() else {
            return Err(crate::decision::ChoiceError::NoDecision);
        };
        let id = d.id.0;
        game.apply_option(option)?;
        self.choices.push((id, option));
        Ok(())
    }

    pub fn finish(self, game: &Game, setup: GameSetup) -> Replay {
        Replay {
            format: REPLAY_FORMAT.into(),
            engine: env!("CARGO_PKG_VERSION").into(),
            pool_hash: pool_hash(game.card_pool()),
            setup,
            choices: self.choices,
            final_hash: game.state_hash(),
            result: game.result().copied(),
        }
    }
}

impl Replay {
    /// Replays against `pool`, returning the final game or the first divergence.
    pub fn run(&self, pool: Arc<CardPool>) -> Result<Game, Divergence> {
        let actual = pool_hash(&pool);
        if actual != self.pool_hash {
            return Err(Divergence::Pool {
                expected: self.pool_hash,
                actual,
            });
        }
        let mut game =
            Game::new(pool, &self.setup).map_err(|e| Divergence::Setup(e.to_string()))?;
        for (index, &(id, option)) in self.choices.iter().enumerate() {
            let pending = game.pending().map(|d| d.id.0);
            if pending != Some(id) {
                return Err(Divergence::Decision {
                    index,
                    expected: id,
                    actual: pending,
                });
            }
            game.apply(Choice {
                decision: DecisionId(id),
                option,
            })
            .map_err(|e| Divergence::Rejected {
                index,
                option,
                error: e.to_string(),
            })?;
        }
        let actual = game.state_hash();
        if actual != self.final_hash {
            return Err(Divergence::FinalHash {
                expected: self.final_hash,
                actual,
            });
        }
        Ok(game)
    }
}
