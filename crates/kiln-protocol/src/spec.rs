//! The experiment spec, `kiln-spec/1` (`docs/13 Simulation/Experiment Spec.md`,
//! ADR-0011). Every block has defaults, so a spec names only what it changes.
//! Unknown keys are errors ([`crate::parse_spec`] suggests the closest name).

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The spec format version this crate reads.
pub const SPEC_VERSION: &str = "kiln-spec/1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    /// Always `kiln-spec/1`.
    pub spec: String,
    /// The run's name; also the default run directory.
    pub name: String,
    /// Rules variant and deck validation. Only `commander` for now.
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default)]
    pub mode: Mode,
    /// Games to play (rounded up for balanced seating).
    #[serde(default = "default_games")]
    pub games: u64,
    /// The experiment seed; each game's seed is derived from it and the game index.
    #[serde(default)]
    pub seed: u64,
    #[serde(default)]
    pub seating: Seating,
    /// Whether policies may assume opponents' lists are known (reported; no
    /// current policy uses it).
    #[serde(default)]
    pub open_decklists: bool,
    #[serde(default)]
    pub decks: Vec<DeckSpec>,
    /// Policies by deck id, plus `default` for every other deck.
    #[serde(default)]
    pub policies: BTreeMap<String, PolicySpec>,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub metrics: Metrics,
    #[serde(default)]
    pub outputs: Outputs,
    /// `mode: goldfish` settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goldfish: Option<Goldfish>,
    /// `mode: ab`: named variants of decks.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variants: BTreeMap<String, Variant>,
    /// `mode: ab`: the same seeds for every variant (common random numbers).
    #[serde(default = "yes")]
    pub paired: bool,
    /// `mode: opening_hands` settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opening_hands: Option<OpeningHands>,
}

fn default_format() -> String {
    "commander".into()
}

fn default_games() -> u64 {
    1000
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Every deck in one pod, one seat each.
    #[default]
    Pod,
    /// One deck against passive opponents: how fast does it win?
    Goldfish,
    /// Variants of a deck against the same field, with paired seeds.
    Ab,
    /// Deal and mulligan only; no game is played.
    OpeningHands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Seating {
    /// Deck i in seat i every game.
    Fixed,
    /// A seeded shuffle per game.
    Random,
    /// The N cyclic rotations, each equally often (the default: seat order
    /// matters a lot in cEDH).
    #[default]
    BalancedRotation,
    /// All N! orders, each equally often (also balances neighbours).
    AllPermutations,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckSpec {
    pub id: String,
    pub source: DeckSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum DeckSource {
    /// A decklist file, relative to the spec's directory.
    File(String),
    /// A decklist pasted inline.
    Text(String),
    /// A Moxfield text export (the same text format).
    MoxfieldExport(String),
}

/// Who makes a deck's decisions (`docs/14 Players & Policies/Policy Composition.md`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PolicySpec {
    /// Uniform over legal options, passing priority with this chance per mille.
    Random {
        #[serde(default = "default_pass")]
        pass: u64,
    },
    /// The greedy baseline (the default).
    #[default]
    Greedy,
    /// Always passes and declines (goldfish opponents).
    Passive,
    /// Ask each in turn; the first that answers decides (the last must
    /// always answer: random, greedy, passive or a playbook).
    Compose { chain: Vec<PolicySpec> },
    /// A deck playbook (a Rhai script, relative to the spec); decisions it
    /// doesn't answer go to `fallback` (greedy by default).
    Playbook {
        playbook: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fallback: Option<Box<PolicySpec>>,
    },
    /// Search AI (M9).
    Search {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        budget: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        playbook: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        playbook_hints: Option<bool>,
    },
    /// A language model (M10).
    Llm {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
}

fn default_pass() -> u64 {
    500
}

impl PolicySpec {
    /// The kind's name, as written in a spec.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Random { .. } => "random",
            Self::Greedy => "greedy",
            Self::Passive => "passive",
            Self::Compose { .. } => "compose",
            Self::Playbook { .. } => "playbook",
            Self::Search { .. } => "search",
            Self::Llm { .. } => "llm",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// Games past this turn end as capped (not a rule).
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    /// Games past this many decisions end as capped (not a rule).
    #[serde(default = "default_max_decisions")]
    pub max_decisions: u64,
    /// Wall-clock cap per game. Off by default: with it, results can depend
    /// on the machine (ADR-0005), and the report warns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_wall_ms: Option<u64>,
    #[serde(default)]
    pub coverage: CoverageGate,
}

fn default_max_turns() -> u32 {
    100
}

fn default_max_decisions() -> u64 {
    200_000
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_turns: default_max_turns(),
            max_decisions: default_max_decisions(),
            max_wall_ms: None,
            coverage: CoverageGate::default(),
        }
    }
}

/// What to do about cards that aren't `verified` (FR-S8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoverageGate {
    /// Refuse to run.
    #[default]
    RequireVerified,
    /// Run, with a warning listing them.
    Warn,
    /// Run.
    Allow,
}

/// Metric categories, each an independent flag (`docs/13 Simulation/Metrics Catalog.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Metrics {
    #[serde(default = "yes")]
    pub win_rates: bool,
    #[serde(default = "yes")]
    pub game_shape: bool,
    #[serde(default = "yes")]
    pub mulligans: bool,
    #[serde(default)]
    pub per_card: PerCard,
    #[serde(default)]
    pub interaction: bool,
    #[serde(default)]
    pub mana: bool,
    /// Wall-clock engine and policy time: not deterministic, so reported apart.
    #[serde(default)]
    pub timing: bool,
    #[serde(default)]
    pub decision_stats: bool,
    /// Every game's full event stream (large).
    #[serde(default)]
    pub raw_events: bool,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            win_rates: true,
            game_shape: true,
            mulligans: true,
            per_card: PerCard::default(),
            interaction: false,
            mana: false,
            timing: false,
            decision_stats: false,
            raw_events: false,
        }
    }
}

impl Metrics {
    /// Whether a category needs the engine's event log.
    pub fn needs_events(&self) -> bool {
        self.per_card.enabled || self.interaction || self.mana || self.raw_events
    }
}

/// Per-card statistics (heavy). `true`/`false`, or the full form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(from = "PerCardInput")]
pub struct PerCard {
    pub enabled: bool,
    /// Deck ids to track; empty means every deck.
    pub decks: Vec<String>,
    /// Cards seen in fewer games are listed without rates.
    pub min_samples: u64,
}

impl Default for PerCard {
    fn default() -> Self {
        Self {
            enabled: false,
            decks: Vec::new(),
            min_samples: 200,
        }
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(untagged)]
enum PerCardInput {
    Flag(bool),
    Full(PerCardFull),
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PerCardFull {
    #[serde(default = "yes")]
    enabled: bool,
    #[serde(default)]
    decks: Vec<String>,
    #[serde(default = "default_min_samples")]
    min_samples: u64,
}

fn default_min_samples() -> u64 {
    200
}

impl From<PerCardInput> for PerCard {
    fn from(i: PerCardInput) -> Self {
        match i {
            PerCardInput::Flag(enabled) => Self {
                enabled,
                ..Self::default()
            },
            PerCardInput::Full(f) => Self {
                enabled: f.enabled,
                decks: f.decks,
                min_samples: f.min_samples,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Outputs {
    /// The run directory; `runs/<name>` if omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    #[serde(default)]
    pub summary: SummaryFormat,
    #[serde(default)]
    pub tables: TableFormat,
    #[serde(default)]
    pub replays: Replays,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SummaryFormat {
    #[default]
    Json,
    Markdown,
}

/// Bulk tables. Parquet is deferred (open question Q38): DuckDB, polars and
/// pandas read JSONL directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TableFormat {
    #[default]
    Jsonl,
    Csv,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Replays {
    /// `none`, `all`, `interesting`, or `sample:N` (every N-th game).
    #[serde(default = "default_keep")]
    pub keep: String,
    /// At most this many replays.
    #[serde(default = "default_replay_max")]
    pub max: u64,
}

fn default_keep() -> String {
    "interesting".into()
}

fn default_replay_max() -> u64 {
    50
}

impl Default for Replays {
    fn default() -> Self {
        Self {
            keep: default_keep(),
            max: default_replay_max(),
        }
    }
}

/// Which replays to keep, parsed from [`Replays::keep`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayKeep {
    None,
    All,
    Interesting,
    /// Every N-th game.
    Sample(u64),
}

impl ReplayKeep {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Self::None),
            "all" => Some(Self::All),
            "interesting" => Some(Self::Interesting),
            _ => s
                .strip_prefix("sample:")
                .and_then(|n| n.parse().ok())
                .filter(|n| *n > 0)
                .map(Self::Sample),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Goldfish {
    /// Passive opponents (each with a deck of basic lands).
    #[serde(default = "default_opponents")]
    pub opponents: u8,
    #[serde(default)]
    pub opponents_behavior: OpponentsBehavior,
    /// Starting life for everyone; the format's default if omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub life: Option<i64>,
}

fn default_opponents() -> u8 {
    3
}

impl Default for Goldfish {
    fn default() -> Self {
        Self {
            opponents: default_opponents(),
            opponents_behavior: OpponentsBehavior::default(),
            life: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OpponentsBehavior {
    #[default]
    Passive,
}

/// An A/B variant: a deck with some cards swapped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Variant {
    /// The deck id it changes.
    pub base: String,
    #[serde(default)]
    pub swap: Swap,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Swap {
    #[serde(default)]
    pub out: Vec<String>,
    #[serde(default, rename = "in")]
    pub add: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpeningHands {
    /// Named card groups to look for in kept hands ("fast_mana: [Sol Ring, …]").
    #[serde(default)]
    pub features: BTreeMap<String, Vec<String>>,
}
