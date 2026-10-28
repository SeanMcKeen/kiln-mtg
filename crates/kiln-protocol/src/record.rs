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
    /// Per-card facts for tracked decks (`metrics.per_card`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cards: Option<Vec<SeatCards>>,
    /// Per seat (`metrics.interaction`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction: Option<Vec<SeatInteraction>>,
    /// Per seat (`metrics.mana`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mana: Option<Vec<SeatMana>>,
    /// `mode: opening_hands`: the hand seat 1 kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand: Option<Vec<String>>,
    /// Per seat: which policy component answered, playbook tags and errors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_stats: Option<Vec<BTreeMap<String, u64>>>,
    /// The first playbook runtime errors ("seat 2: priority: …").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_errors: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SeatCards {
    pub seat: usize,
    pub cards: Vec<CardFact>,
}

/// One card its owner saw (commanders and tokens aren't tracked).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CardFact {
    pub name: String,
    /// The turn it first reached its owner's hand; 0 = the opening hand.
    pub seen: u32,
    /// The turn it was first cast or played.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cast: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SeatInteraction {
    /// Spells cast that counter spells or abilities.
    pub counterspells: u32,
    /// Its spells that were countered.
    pub countered: u32,
    /// Times its spells and abilities targeted a permanent another player controlled.
    pub targeted_opponents: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SeatMana {
    pub lands_played: u32,
    /// Mana added to its pool (each unit).
    pub mana_produced: u32,
    /// Of its first five turns, how many it took.
    pub early_turns: u32,
    /// Of those, turns with no land played.
    pub missed_land_drops: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Win,
    Draw,
    /// Stopped by a harness limit; excluded from win-rate denominators.
    Capped,
    /// No game was played (`mode: opening_hands`).
    NotPlayed,
    /// `mode: goldfish`: the measured deck left the game, which the passive
    /// opponents could never finish. A loss for it, in win-rate denominators.
    Lost,
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
    /// Where the spec's relative deck paths start (for `kiln game inspect`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec_dir: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Complete,
    Cancelled,
}
