//! The report, `kiln-report/1` (`docs/13 Simulation/Output Formats & Report
//! Schema.md`). [`Report`] is the detailed form; [`ConciseReport`] is the
//! agent-facing summary (≈ ≤ 2k tokens). Every rate has a 95% interval.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const REPORT_VERSION: &str = "kiln-report/1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub report: String,
    pub run_id: String,
    pub mode: String,
    pub versions: Versions,
    /// Threats to validity, most important first (Validity & Caveats).
    pub warnings: Vec<String>,
    pub configs: Vec<ConfigReport>,
    /// A/B differences from the base configuration (mode: ab).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ab: Vec<AbComparison>,
    /// Opening-hand statistics (mode: opening_hands).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opening_hands: Vec<OpeningHandStats>,
    pub notable: Vec<String>,
    /// Suggested follow-up commands.
    pub next: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Versions {
    pub engine: String,
    pub card_db: String,
    pub cr: String,
    pub spec_hash: String,
}

/// A rate with its 95% Wilson interval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rate {
    pub n: u64,
    pub k: u64,
    pub rate: f64,
    pub ci95: [f64; 2],
}

/// A distribution of whole numbers (turns).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Dist {
    pub n: u64,
    pub mean: f64,
    pub median: u32,
    /// 95% bootstrap interval for the median.
    pub median_ci95: [u32; 2],
    pub p10: u32,
    pub p90: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigReport {
    pub config: String,
    pub games: u64,
    /// Games stopped by a limit; not in win-rate denominators.
    pub capped: Rate,
    pub draws: Rate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub win_rates: Option<WinRates>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_shape: Option<GameShape>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mulligans: Vec<DeckMulligans>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_stats: Option<DecisionStats>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_card: Option<PerCardReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction: Option<BTreeMap<String, InteractionStats>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mana: Option<BTreeMap<String, ManaStats>>,
    /// Wall-clock time: not deterministic (left out of bit-identical comparisons).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<TimingStats>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WinRates {
    pub decks: Vec<DeckWinRate>,
    /// Pooled over decks: the win rate of each seat in turn order.
    pub by_seat: Vec<Rate>,
    /// loser deck → winner deck → games ("who won when X lost").
    pub head_to_head: BTreeMap<String, BTreeMap<String, u64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckWinRate {
    pub deck: String,
    pub win: Rate,
    /// Win rate minus 1/N (the even share in an N-player pod).
    pub vs_baseline: f64,
    /// The deck's win rate in each seat.
    pub by_seat: Vec<Rate>,
    /// Finishing position counts: [first out, second out, …, winner].
    pub finish: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GameShape {
    pub turns: Dist,
    /// Rounds (turns ÷ players, rounded up).
    pub rounds: Dist,
    pub decks: Vec<DeckShape>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckShape {
    pub deck: String,
    /// Its own turn number when it won (its Nth turn, as in "a turn-4 win").
    pub win_turn: Option<Dist>,
    /// Win type → share of its wins.
    pub win_types: BTreeMap<String, Rate>,
    /// Its own turn number when it left games it lost.
    pub loss_turn: Option<Dist>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckMulligans {
    pub deck: String,
    /// Share of games kept at 7, 6, 5, … cards (index = mulligans taken).
    pub kept_at: Vec<Rate>,
    /// Win rate by mulligans taken.
    pub win_rate_by_mulligans: Vec<Rate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DecisionStats {
    pub decisions_per_game: Dist,
    /// Decision kind → total over all games.
    pub by_kind: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimingStats {
    pub note: String,
    pub games_per_second_per_core: f64,
    pub mean_game_ms: f64,
    /// Mean share of time spent in policies, per seat.
    pub policy_share_by_seat: Vec<f64>,
}

/// Per-card statistics (correlational; Metrics Catalog `per_card`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PerCardReport {
    pub note: String,
    pub decks: Vec<DeckCards>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckCards {
    pub deck: String,
    pub games: u64,
    pub cards: Vec<CardStats>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CardStats {
    pub name: String,
    /// Games it was in the opening hand or drawn.
    pub seen: Rate,
    /// Of games seen, the share it was cast or played.
    pub cast: Rate,
    /// Games-in-hand win rate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gih_wr: Option<Rate>,
    /// Opening-hand win rate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oh_wr: Option<Rate>,
    /// GIH WR minus the win rate when not seen (confounded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iwd: Option<f64>,
    /// Benjamini–Hochberg q-value for IWD ≠ 0 across the deck's cards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iwd_q: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cast_wr: Option<Rate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_cast_turn: Option<Dist>,
    /// Seen but never cast or played.
    pub dead: Rate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InteractionStats {
    pub counterspells_cast_per_game: f64,
    /// Its spells countered, per game.
    pub spells_countered_per_game: f64,
    pub removal_targets_per_game: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManaStats {
    /// Lands played per game it was in.
    pub lands_played_per_game: f64,
    /// Mana produced per game (all sources).
    pub mana_produced_per_game: f64,
    /// Share of its turns with no land played, through turn 5.
    pub missed_land_drops: Rate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AbComparison {
    pub variant: String,
    pub base_deck: String,
    pub paired_games: u64,
    pub base: Rate,
    pub variant_rate: Rate,
    /// Variant minus base win rate, with a 95% paired-bootstrap interval.
    pub difference: f64,
    pub difference_ci95: [f64; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpeningHandStats {
    pub deck: String,
    pub hands: u64,
    /// Share of hands kept at 7, 6, 5, …
    pub kept_at: Vec<Rate>,
    /// Feature name → share of kept hands with at least one of its cards.
    pub features: BTreeMap<String, Rate>,
}

/// The agent-facing summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConciseReport {
    pub report: String,
    pub run_id: String,
    pub mode: String,
    pub games: u64,
    pub capped: u64,
    pub versions: Versions,
    pub warnings: Vec<String>,
    /// Per configuration: deck → win rate, interval and by-seat rates.
    pub win_rates: BTreeMap<String, Vec<ConciseWinRate>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub median_win_turn: BTreeMap<String, BTreeMap<String, u32>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub win_types: BTreeMap<String, BTreeMap<String, BTreeMap<String, f64>>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub keep_at_7: BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ab: Vec<AbComparison>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opening_hands: Vec<OpeningHandStats>,
    pub notable: Vec<String>,
    pub next: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConciseWinRate {
    pub deck: String,
    pub win_rate: f64,
    pub ci95: [f64; 2],
    pub by_seat: Vec<f64>,
}
