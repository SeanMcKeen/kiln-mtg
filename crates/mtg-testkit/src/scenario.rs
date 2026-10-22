//! Scenario files (`docs/08 Testing/Scenario File Format.md`), M2 version.
//!
//! A scenario sets up a board, plays a script of choices matched by **option
//! label**, and checks expectations. The same format serves tests, the judge
//! tool and the MCP server later.
//!
//! ```yaml
//! scenario: mtg-scenario/1
//! name: First strike kills the blocker first
//! cr: ["510.4", "702.7b"]
//! players:
//!   - battlefield: [Test Knight]
//!   - battlefield: [Test Bear]
//! turn: { number: 3, active: P1, step: declare_attackers }
//! script:
//!   - choose: Test Knight attacks P2
//!   - pass_all
//!   - { player: P2, choose: Test Bear blocks Test Knight (P1) }
//!   - pass_all
//!   - pass_all
//! expect:
//!   zones: { P2.graveyard: [Test Bear] }
//! ```

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use mtg_core::PlayerId;
use mtg_engine::{
    CardPool, DecisionKind, Game, GameBuilder, GameResult, PermanentOpts, Step, ZoneId,
};
use serde::Deserialize;

pub const SCENARIO_FORMAT: &str = "mtg-scenario/1";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub scenario: String,
    pub name: String,
    /// CR rules the scenario demonstrates.
    #[serde(default)]
    pub cr: Vec<String>,
    /// Real cards this scenario verifies (`tests/scenarios-real/`, M3 plan B-D13).
    #[serde(default)]
    pub verifies: Vec<String>,
    pub players: Vec<PlayerSpec>,
    #[serde(default)]
    pub turn: TurnSpec,
    #[serde(default)]
    pub seed: u64,
    #[serde(default)]
    pub script: Vec<ScriptStep>,
    #[serde(default)]
    pub expect: Expect,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerSpec {
    pub life: Option<i64>,
    #[serde(default)]
    pub hand: Vec<String>,
    /// Top card first. Omitted = 20 filler lands ("Test Wastes"); `[]` = empty.
    pub library: Option<Vec<String>>,
    #[serde(default)]
    pub graveyard: Vec<String>,
    #[serde(default)]
    pub battlefield: Vec<PermSpec>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PermSpec {
    Name(String),
    Full {
        card: String,
        #[serde(default)]
        tapped: bool,
        #[serde(default)]
        damage: u32,
        #[serde(default)]
        sick: bool,
        /// Counter kind (`+1/+1`, `age`, ...) → count.
        #[serde(default)]
        counters: BTreeMap<String, u32>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnSpec {
    #[serde(default = "default_turn")]
    pub number: u32,
    #[serde(default = "default_active")]
    pub active: String,
    #[serde(default = "default_step")]
    pub step: String,
}

impl Default for TurnSpec {
    fn default() -> Self {
        Self {
            number: default_turn(),
            active: default_active(),
            step: default_step(),
        }
    }
}

fn default_turn() -> u32 {
    3
}
fn default_active() -> String {
    "P1".into()
}
fn default_step() -> String {
    "precombat_main".into()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ScriptStep {
    /// `pass_all`: every player in the game passes once in succession (CR 117.4).
    Keyword(String),
    Choose {
        choose: String,
        #[serde(default)]
        player: Option<String>,
    },
    /// Pass priority `n` times.
    Pass { pass: u32 },
    /// Check expectations mid-script.
    Check { check: Box<Expect> },
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    /// `in_progress`, `draw`, or `win P2`.
    pub result: Option<String>,
    #[serde(default)]
    pub life: BTreeMap<String, i64>,
    /// `P1.hand`, `P2.graveyard`, `P1.library`, `P3.battlefield` (permanents P3 controls), `stack`, `exile`.
    /// Compared as multisets of card names.
    #[serde(default)]
    pub zones: BTreeMap<String, Vec<String>>,
    /// `P1` → the top cards of that player's library, top first, in order.
    #[serde(default)]
    pub library_top: BTreeMap<String, Vec<String>>,
    /// `P1` → that player's mana pool as symbols in WUBRGC order, e.g. `"{B}{B}{B}"` (`""` = empty).
    #[serde(default)]
    pub pool: BTreeMap<String, String>,
    /// Card name → counter kind (`+1/+1`, `-1/-1`, `charge`, `loyalty`) →
    /// count, for the first permanent with that name. Unlisted kinds aren't checked.
    #[serde(default)]
    pub counters: BTreeMap<String, BTreeMap<String, u32>>,
    /// Card name → expected tapped state, for the first permanent with that name.
    #[serde(default)]
    pub tapped: BTreeMap<String, bool>,
    /// Card name → `[power, toughness]` after the layers (CR 613), or `null`
    /// for a noncreature, for the first permanent with that name.
    #[serde(default)]
    pub pt: BTreeMap<String, Option<[i32; 2]>>,
    pub step: Option<String>,
    pub decision: Option<DecisionExpect>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionExpect {
    pub player: Option<String>,
    /// Snake-case kind, e.g. `priority`, `declare_blocker`, `assign_damage`.
    pub kind: Option<String>,
    #[serde(default)]
    pub options_include: Vec<String>,
    #[serde(default)]
    pub options_exclude: Vec<String>,
    /// The exact option labels, in order.
    pub options: Option<Vec<String>>,
}

#[derive(Debug, thiserror::Error)]
pub enum ScenarioError {
    #[error("can't read scenario {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{path}: invalid scenario YAML: {source}")]
    Yaml {
        path: String,
        source: yaml_serde::Error,
    },
    #[error("{0}")]
    Setup(String),
    #[error("script step {step} ({what}): {message}")]
    Script {
        step: usize,
        what: String,
        message: String,
    },
    #[error("expectation failed{at}: {message}")]
    Expect { at: String, message: String },
}

pub fn load(path: &Path) -> Result<Scenario, ScenarioError> {
    let text = std::fs::read_to_string(path).map_err(|source| ScenarioError::Io {
        path: path.display().to_string(),
        source,
    })?;
    parse(&text, &path.display().to_string())
}

pub fn parse(text: &str, label: &str) -> Result<Scenario, ScenarioError> {
    yaml_serde::from_str(text).map_err(|source| ScenarioError::Yaml {
        path: label.to_owned(),
        source,
    })
}

fn player_id(s: &str, n: usize) -> Result<PlayerId, String> {
    let i: usize = s
        .strip_prefix('P')
        .and_then(|x| x.parse().ok())
        .ok_or_else(|| format!("{s:?} isn't a player (use P1..P{n})"))?;
    if i == 0 || i > n {
        return Err(format!("{s:?} isn't a player in a {n}-player scenario"));
    }
    Ok(PlayerId(u8::try_from(i - 1).unwrap_or(0)))
}

fn step_of(s: &str) -> Result<Step, String> {
    Ok(match s {
        "untap" => Step::Untap,
        "upkeep" => Step::Upkeep,
        "draw" => Step::Draw,
        "precombat_main" | "main1" => Step::PrecombatMain,
        "beginning_of_combat" | "begin_combat" => Step::BeginCombat,
        "declare_attackers" => Step::DeclareAttackers,
        "postcombat_main" | "main2" => Step::PostcombatMain,
        "end" => Step::End,
        "cleanup" => Step::Cleanup,
        "declare_blockers" | "first_strike_damage" | "combat_damage" | "end_of_combat" => {
            return Err(format!(
                "scenarios can't start at {s}; start at declare_attackers and script the attacks"
            ));
        }
        other => return Err(format!("unknown step {other:?}")),
    })
}

fn step_name(s: Step) -> &'static str {
    match s {
        Step::Untap => "untap",
        Step::Upkeep => "upkeep",
        Step::Draw => "draw",
        Step::PrecombatMain => "precombat_main",
        Step::BeginCombat => "beginning_of_combat",
        Step::DeclareAttackers => "declare_attackers",
        Step::DeclareBlockers => "declare_blockers",
        Step::FirstStrikeDamage => "first_strike_damage",
        Step::CombatDamage => "combat_damage",
        Step::EndCombat => "end_of_combat",
        Step::PostcombatMain => "postcombat_main",
        Step::End => "end",
        Step::Cleanup => "cleanup",
    }
}

/// Snake-case name of a decision kind.
pub fn kind_name(k: &DecisionKind) -> &'static str {
    match k {
        DecisionKind::Mulligan { .. } => "mulligan",
        DecisionKind::BottomCard { .. } => "bottom_card",
        DecisionKind::Priority { .. } => "priority",
        DecisionKind::ChooseTarget { .. } => "choose_target",
        DecisionKind::PayMana { .. } => "pay_mana",
        DecisionKind::ManaStep { .. } => "mana_step",
        DecisionKind::DeclareAttacker { .. } => "declare_attacker",
        DecisionKind::DeclareBlocker { .. } => "declare_blocker",
        DecisionKind::AssignDamage { .. } => "assign_damage",
        DecisionKind::Discard { .. } => "discard",
        DecisionKind::LegendRule { .. } => "legend_rule",
        DecisionKind::Optional { .. } => "optional",
        DecisionKind::ChooseCard { .. } => "choose_card",
        DecisionKind::ChooseMana { .. } => "choose_mana",
        DecisionKind::ChooseName { .. } => "choose_name",
        DecisionKind::ChooseNumber { .. } => "choose_number",
        DecisionKind::ChooseColor { .. } => "choose_color",
        DecisionKind::ChooseMode { .. } => "choose_mode",
        DecisionKind::OrderTriggers { .. } => "order_triggers",
        DecisionKind::ChooseReplacement { .. } => "choose_replacement",
        DecisionKind::ChangeTarget { .. } => "change_target",
        DecisionKind::ProposeShortcut { .. } => "propose_shortcut",
        DecisionKind::RespondShortcut { .. } => "respond_shortcut",
        DecisionKind::ChooseCost { .. } => "choose_cost",
        DecisionKind::ChooseSymbolPay { .. } => "choose_symbol_pay",
    }
}

/// All option labels of the pending decision.
pub fn labels(game: &Game) -> Vec<String> {
    let n = game
        .pending()
        .map_or(0, mtg_engine::PendingDecision::option_count);
    (0..n).filter_map(|i| game.option_label(i)).collect()
}

/// Builds the scenario's starting game.
pub fn build(s: &Scenario, pool: &Arc<CardPool>) -> Result<Game, ScenarioError> {
    if s.scenario != SCENARIO_FORMAT {
        return Err(ScenarioError::Setup(format!(
            "unsupported format {:?}; expected {SCENARIO_FORMAT}",
            s.scenario
        )));
    }
    let n = s.players.len();
    if !(2..=8).contains(&n) {
        return Err(ScenarioError::Setup(format!(
            "{n} players; scenarios need 2 to 8"
        )));
    }
    let card = |name: &str| {
        pool.by_name(name).ok_or_else(|| {
            ScenarioError::Setup(format!("unknown card {name:?} (not in the card pool)"))
        })
    };
    let cards = |names: &[String]| names.iter().map(|x| card(x)).collect::<Result<Vec<_>, _>>();
    let mut b = GameBuilder::new(Arc::clone(pool), n).seed(s.seed);
    for (i, p) in s.players.iter().enumerate() {
        let id = PlayerId(u8::try_from(i).unwrap_or(0));
        if let Some(l) = p.life {
            b = b.life(id, l);
        }
        let library = match &p.library {
            Some(l) => cards(l)?,
            None => vec![card("Test Wastes")?; 20],
        };
        b = b
            .library(id, library)
            .hand(id, cards(&p.hand)?)
            .graveyard(id, cards(&p.graveyard)?);
        for perm in &p.battlefield {
            let (name, opts) = match perm {
                PermSpec::Name(n) => (n.as_str(), PermanentOpts::default()),
                PermSpec::Full {
                    card,
                    tapped,
                    damage,
                    sick,
                    counters,
                } => (
                    card.as_str(),
                    PermanentOpts {
                        tapped: *tapped,
                        damage: *damage,
                        summoning_sick: *sick,
                        counters: counters
                            .iter()
                            .map(|(k, n)| {
                                serde_json::from_value(serde_json::Value::String(k.clone()))
                                    .map(|k| (k, *n))
                                    .map_err(|_| {
                                        ScenarioError::Setup(format!("unknown counter kind {k:?}"))
                                    })
                            })
                            .collect::<Result<_, _>>()?,
                    },
                ),
            };
            b = b.permanent(id, card(name)?, opts);
        }
    }
    let active = player_id(&s.turn.active, n).map_err(ScenarioError::Setup)?;
    let step = step_of(&s.turn.step).map_err(ScenarioError::Setup)?;
    Ok(b.turn(s.turn.number, active, step).build())
}

/// Runs a scenario: build, play the script, check expectations.
pub fn run(s: &Scenario, pool: &Arc<CardPool>) -> Result<Game, ScenarioError> {
    let mut game = build(s, pool)?;
    let n = s.players.len();
    for (i, step) in s.script.iter().enumerate() {
        let script_err = |what: &str, message: String| ScenarioError::Script {
            step: i + 1,
            what: what.to_owned(),
            message,
        };
        match step {
            ScriptStep::Keyword(k) if k == "pass_all" => {
                let count = (0..n)
                    .filter(|p| {
                        game.player_state(PlayerId(u8::try_from(*p).unwrap_or(0)))
                            .in_game()
                    })
                    .count();
                for _ in 0..count {
                    pass_once(&mut game).map_err(|m| script_err("pass_all", m))?;
                }
            }
            ScriptStep::Keyword(k) => {
                return Err(script_err(k, "unknown keyword; use pass_all".into()));
            }
            ScriptStep::Pass { pass } => {
                for _ in 0..*pass {
                    pass_once(&mut game).map_err(|m| script_err("pass", m))?;
                }
            }
            ScriptStep::Choose { choose, player } => {
                let Some(d) = game.pending() else {
                    return Err(script_err(choose, "the game is over".into()));
                };
                if let Some(p) = player {
                    let want = player_id(p, n).map_err(|m| script_err(choose, m))?;
                    if d.player != want {
                        return Err(script_err(
                            choose,
                            format!("decision belongs to {}, not {p}", d.player),
                        ));
                    }
                }
                let all = labels(&game);
                let Some(idx) = all.iter().position(|l| l == choose) else {
                    return Err(script_err(
                        choose,
                        format!("no option with that label; options are {all:?}"),
                    ));
                };
                game.apply_option(u32::try_from(idx).unwrap_or(0))
                    .map_err(|e| script_err(choose, e.to_string()))?;
            }
            ScriptStep::Check { check } => {
                check_expect(&game, check, &format!(" at script step {}", i + 1))?;
            }
        }
    }
    check_expect(&game, &s.expect, "")?;
    Ok(game)
}

fn pass_once(game: &mut Game) -> Result<(), String> {
    let Some(d) = game.pending() else {
        return Err("the game is over".into());
    };
    if !matches!(d.kind, DecisionKind::Priority { .. }) {
        return Err(format!(
            "expected a priority decision, got {} for {}; options {:?}",
            kind_name(&d.kind),
            d.player,
            labels(game)
        ));
    }
    game.apply_option(0).map_err(|e| e.to_string())
}

fn check_expect(game: &Game, e: &Expect, at: &str) -> Result<(), ScenarioError> {
    let fail = |message: String| ScenarioError::Expect {
        at: at.to_owned(),
        message,
    };
    let n = game.player_count();
    if let Some(r) = &e.result {
        let actual = match game.result() {
            None => "in_progress".to_owned(),
            Some(GameResult::Draw { .. }) => "draw".to_owned(),
            Some(GameResult::Win { player, .. }) => format!("win {player}"),
            Some(GameResult::Aborted { .. }) => "aborted".to_owned(),
        };
        if &actual != r {
            return Err(fail(format!("result is {actual}, expected {r}")));
        }
    }
    for (p, life) in &e.life {
        let id = player_id(p, n).map_err(fail)?;
        let actual = game.player_state(id).life;
        if actual != *life {
            return Err(fail(format!("{p} life is {actual}, expected {life}")));
        }
    }
    for (key, want) in &e.zones {
        let ids = match key.split_once('.') {
            Some((p, zone)) => {
                let id = player_id(p, n).map_err(fail)?;
                match zone {
                    "hand" => game.objects_in(ZoneId::Hand(id)),
                    "library" => game.objects_in(ZoneId::Library(id)),
                    "graveyard" => game.objects_in(ZoneId::Graveyard(id)),
                    "battlefield" => game
                        .objects_in(ZoneId::Battlefield)
                        .into_iter()
                        .filter(|o| game.object(*o).is_some_and(|x| x.controller == id))
                        .collect(),
                    other => return Err(fail(format!("unknown zone {other:?} in {key:?}"))),
                }
            }
            None => match key.as_str() {
                "stack" => game.objects_in(ZoneId::Stack),
                "exile" => game.objects_in(ZoneId::Exile),
                "battlefield" => game.objects_in(ZoneId::Battlefield),
                other => {
                    return Err(fail(format!(
                        "unknown zone key {other:?}; use e.g. P1.graveyard"
                    )));
                }
            },
        };
        let mut actual: Vec<String> = ids
            .iter()
            .filter_map(|id| game.object_name(*id).map(str::to_owned))
            .collect();
        let mut want = want.clone();
        actual.sort();
        want.sort();
        if actual != want {
            return Err(fail(format!("{key} is {actual:?}, expected {want:?}")));
        }
    }
    for (p, want) in &e.library_top {
        let id = player_id(p, n).map_err(fail)?;
        let actual: Vec<String> = game
            .objects_in(ZoneId::Library(id))
            .iter()
            .rev()
            .take(want.len())
            .filter_map(|o| game.object_name(*o).map(str::to_owned))
            .collect();
        if &actual != want {
            return Err(fail(format!(
                "{p}'s library top is {actual:?}, expected {want:?}"
            )));
        }
    }
    for (p, want) in &e.pool {
        let id = player_id(p, n).map_err(fail)?;
        let pool = game.player_state(id).pool;
        let mut actual = String::new();
        for t in mtg_engine::ManaType::ALL {
            for _ in 0..pool.get(t) {
                actual.push_str(t.symbol());
            }
        }
        if &actual != want {
            return Err(fail(format!(
                "{p}'s mana pool is {actual:?}, expected {want:?}"
            )));
        }
    }
    for (name, want) in &e.pt {
        let found = game
            .objects_in(ZoneId::Battlefield)
            .into_iter()
            .find(|id| game.object_name(*id) == Some(name.as_str()));
        let Some(id) = found else {
            return Err(fail(format!("no permanent named {name:?}")));
        };
        let have = game.power_toughness(id).map(|(p, t)| [p, t]);
        if have != *want {
            return Err(fail(format!(
                "{name} has power/toughness {have:?}, expected {want:?}"
            )));
        }
    }
    for (name, want) in &e.counters {
        let found = game
            .objects_in(ZoneId::Battlefield)
            .into_iter()
            .find(|id| game.object_name(*id) == Some(name.as_str()));
        let Some(id) = found else {
            return Err(fail(format!("no permanent named {name:?}")));
        };
        let have = game.counters_of(id);
        for (kind, n) in want {
            let actual = have
                .iter()
                .find(|(k, _)| {
                    serde_json::to_value(k)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .as_deref()
                        == Some(kind.as_str())
                })
                .map_or(0, |(_, c)| *c);
            if actual != *n {
                return Err(fail(format!(
                    "{name} has {actual} {kind} counters, expected {n}"
                )));
            }
        }
    }
    for (name, tapped) in &e.tapped {
        let found = game
            .objects_in(ZoneId::Battlefield)
            .into_iter()
            .find(|id| game.object_name(*id) == Some(name.as_str()));
        let Some(id) = found else {
            return Err(fail(format!("no permanent named {name:?}")));
        };
        let actual = game.object(id).is_some_and(|o| o.tapped);
        if actual != *tapped {
            return Err(fail(format!("{name} tapped = {actual}, expected {tapped}")));
        }
    }
    if let Some(step) = &e.step {
        let actual = step_name(game.turn().step);
        if actual != step {
            return Err(fail(format!("step is {actual}, expected {step}")));
        }
    }
    if let Some(d) = &e.decision {
        let Some(pending) = game.pending() else {
            return Err(fail("expected a decision, but the game is over".into()));
        };
        if let Some(p) = &d.player {
            let want = player_id(p, n).map_err(fail)?;
            if pending.player != want {
                return Err(fail(format!(
                    "decision belongs to {}, expected {p}",
                    pending.player
                )));
            }
        }
        if let Some(k) = &d.kind
            && kind_name(&pending.kind) != k
        {
            return Err(fail(format!(
                "decision kind is {}, expected {k}",
                kind_name(&pending.kind)
            )));
        }
        let all = labels(game);
        for l in &d.options_include {
            if !all.contains(l) {
                return Err(fail(format!("option {l:?} missing; options are {all:?}")));
            }
        }
        for l in &d.options_exclude {
            if all.contains(l) {
                return Err(fail(format!(
                    "option {l:?} should not be offered; options are {all:?}"
                )));
            }
        }
        if let Some(exact) = &d.options
            && &all != exact
        {
            return Err(fail(format!("options are {all:?}, expected {exact:?}")));
        }
    }
    Ok(())
}
