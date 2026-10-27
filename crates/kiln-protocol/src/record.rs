//! Per-game records (M7 plan S7): one line of `games.jsonl` per finished game.
//! Every report section is computed from these, sorted by game index.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The configuration name of a pod or goldfish run (A/B runs use variant names).
pub const BASE_CONFIG: &str = "base";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GameRecord {
    /// `base`, or the A/B variant this game belongs to.
    pub config: String,
    /// The game's index; configurations share indices and seeds (paired runs).
    pub index: u64,
    pub seed: u64,
    /// The deck id in each seat, in turn order (seat 1 plays first).
    /// Goldfish opponents are `opponent`.
    pub seats: Vec<String>,
    pub outcome: Outcome,
    /// The winning seat (0-based).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winner: Option<usize>,
    /// How the game was won (M7 plan S11).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub win_type: Option<String>,
    /// Turn number when the game ended.
    pub turns: u32,
    /// Players in the order they left the game.
    pub eliminations: Vec<Elimination>,
    /// Mulligans taken, per seat.
    pub mulligans: Vec<u32>,
    pub decisions: u64,
    /// Why a capped game stopped: `max_turns`, `max_decisions` or `max_wall_ms`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capped: Option<String>,
    /// Decision counts by kind (`metrics.decision_stats`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_kinds: Option<BTreeMap<String, u64>>,
    /// Wall-clock time (`metrics.timing`): not deterministic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<Timing>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Win,
    Draw,
    /// Stopped by a harness limit; excluded from win-rate denominators.
    Capped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Elimination {
    pub seat: usize,
    pub turn: u32,
    /// `life`, `commander_damage`, `decking`, `poison`, `concession` or `effect_loss`.
    pub reason: String,
}

/// Microseconds of wall-clock time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Timing {
    pub total_us: u64,
    /// Time inside each seat's policy.
    pub policy_us: Vec<u64>,
}

/// `runs/<id>/run.json`: what a run is and how far it got.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunManifest {
    pub run: String,
    pub spec_hash: String,
    pub card_db: String,
    pub engine: String,
    /// Games over every configuration.
    pub games_total: u64,
    pub games_done: u64,
    pub status: RunStatus,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Complete,
    Cancelled,
}
