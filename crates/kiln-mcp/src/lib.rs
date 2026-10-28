//! `kiln-mcp`: the MCP server that exposes the simulation lab to AI agents
//! (ADR-0008; `docs/07 Interfaces/MCP Server Tool Catalog.md`; M8 plan P6).
//!
//! The tools are thin: every one calls the same `kiln-sim` functions the CLI
//! uses, so the interfaces behave identically. Long runs are asynchronous:
//! `kiln_sim_run` validates the spec and returns a run id at once; the agent
//! polls `kiln_sim_status` and reads `kiln_sim_report`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use kiln_data::{CardDb, NameMatch};
use kiln_engine::CardPool;
use kiln_sim::deck::{ValidateOptions, parse_deck, validate_with};
use kiln_sim::experiment::{
    self, Experiment, Progress, RunOptions, check_playbook, inspect, report,
};
use kiln_sim::library::Library;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

/// What the server needs, built once at startup.
#[derive(Debug)]
pub struct ServerState {
    pub db: CardDb,
    pub lib: Library,
    /// Real cards plus the synthetic test set, for rules scenarios.
    pub scenario_pool: Arc<CardPool>,
    /// Runs, imported decks and playbooks live under here; relative paths in
    /// specs start here too.
    pub workdir: PathBuf,
    /// Worker threads per run unless a call asks for more.
    pub default_threads: usize,
}

#[derive(Debug)]
struct Job {
    dir: PathBuf,
    games_total: u64,
    cancel: Arc<AtomicBool>,
    started: Instant,
    /// `None` while running; then the outcome.
    finished: Arc<Mutex<Option<Result<(), String>>>>,
}

const INSTRUCTIONS: &str = "Kiln simulates Magic: The Gathering Commander games with an exact rules engine. \
Typical flow for 'how does my deck do in a pod': (1) kiln_deck_import each decklist (it reports legality and \
which cards the engine supports; only 'verified' cards run by default); (2) optionally write a playbook (a Rhai \
script that pilots a deck) and check it with kiln_playbook_validate; (3) write a kiln-spec/1 experiment spec \
(YAML) and start it with kiln_sim_run; (4) poll kiln_sim_status until complete; (5) read kiln_sim_report. \
Every rate has a 95% interval: treat differences whose intervals overlap as unresolved, and read the warnings \
(baseline pilots, seat bias, few games). Use mode: ab to test a card swap causally. Pilots are random, greedy, \
passive or playbook-driven; search AI arrives later.";

/// The MCP server.
#[derive(Clone)]
pub struct KilnServer {
    state: Arc<ServerState>,
    jobs: Arc<Mutex<BTreeMap<String, Job>>>,
    tool_router: ToolRouter<Self>,
}

impl std::fmt::Debug for KilnServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KilnServer").finish_non_exhaustive()
    }
}

fn to_text(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

fn slug(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_matches('-').chars().take(48).collect()
}

/// A report's next-step hint (written for the CLI) as the matching tool call.
fn mcp_hint(hint: &str, run_id: &str) -> String {
    if hint.starts_with("kiln sim report ") {
        return format!("kiln_sim_report run_id={run_id} format=detailed");
    }
    if let Some(rest) = hint.strip_prefix("kiln game inspect ") {
        let mut words = rest.split_whitespace();
        if let (Some(_dir), Some(index)) = (words.next(), words.next()) {
            let note = rest.split_once('#').map_or("", |(_, n)| n.trim());
            return format!("kiln_game_inspect run_id={run_id} game_index={index}  ({note})");
        }
    }
    hint.to_owned()
}

/// What to do after a status check.
fn status_next(status: &str, run_id: &str, eta: Option<f64>) -> String {
    match status {
        "complete" => format!("kiln_sim_report run_id={run_id}"),
        "cancelled" => {
            "resume: call kiln_sim_run with the same spec; finished games are kept".to_owned()
        }
        "starting" | "running" | "cancelling" => {
            // About a quarter of the remaining time, between 2 and 30 seconds.
            let wait = eta.map_or(5.0, |e| (e / 4.0).clamp(2.0, 30.0));
            format!("poll kiln_sim_status run_id={run_id} again in about {wait:.0} s")
        }
        _ => "fix the problem, then call kiln_sim_run again".to_owned(),
    }
}

/// Runs blocking work off the async runtime.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("internal error: {e}"))
}

// ---- parameters -----------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeckImportParams {
    /// The decklist: pasted text (one "1 Card Name" per line; a "Commander"
    /// section or *CMDR* marks commanders), or a path to a decklist file.
    pub source: String,
    /// A name for the deck (used for its id and file name).
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CardLookupParams {
    /// Card names (up to 50). Misspellings get suggestions.
    pub names: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SimRunParams {
    /// A kiln-spec/1 experiment spec as YAML or JSON. Minimal pod:
    /// "spec: kiln-spec/1\nname: my-pod\ngames: 2000\ndecks:\n  - { id: a, source: { file: decks/a.txt } }\n  - …\npolicies:\n  default: { kind: greedy }".
    /// Modes: pod (default), goldfish, ab (card swaps on paired seeds), opening_hands.
    /// Deck files are the paths kiln_deck_import returns.
    pub spec_text: String,
    /// Worker threads for this run (default: the server's setting).
    #[serde(default)]
    pub threads: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunIdParams {
    /// The run id kiln_sim_run returned (the spec's name).
    pub run_id: String,
    /// Stop the run after the games in progress (it can be resumed by running the same spec).
    #[serde(default)]
    pub cancel: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SimReportParams {
    pub run_id: String,
    /// `concise` (default, ~2k tokens), `detailed` (JSON, filtered by `sections`), or `markdown`.
    #[serde(default)]
    pub format: Option<String>,
    /// For `detailed`: which sections to include (win_rates, game_shape, mulligans, per_card,
    /// interaction, mana, timing, decision_stats, policies). Default: all but per_card.
    #[serde(default)]
    pub sections: Vec<String>,
    /// For per_card: which page of each deck's cards (1-based, default 1) and how many per page
    /// (default 25). Cards are sorted by how often they were seen.
    #[serde(default)]
    pub page: Option<usize>,
    #[serde(default)]
    pub page_size: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompareParams {
    pub run_a: String,
    pub run_b: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InspectParams {
    pub run_id: String,
    pub game_index: u64,
    /// `base`, or an A/B variant name.
    #[serde(default)]
    pub config: Option<String>,
    /// How many of the last events to return (default 60; 0 = all).
    #[serde(default)]
    pub events: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScenarioParams {
    /// A rules scenario (kiln-scenario/1 YAML): players' zones, an optional turn and step,
    /// a script of choices by label, and expectations. Returns the event log and whether
    /// the expectations held.
    pub scenario: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlaybookParams {
    /// The deck it pilots: a decklist path (as kiln_deck_import returns).
    pub deck: String,
    /// The playbook source (Rhai). Define functions named after decision kinds
    /// (mulligan, priority, choose_target, declare_attacker, …) or `decide`, each taking
    /// (view, decision) and returning an option index or label, or () to defer.
    pub playbook_text: String,
    /// A name for the playbook file.
    #[serde(default)]
    pub name: Option<String>,
    /// Goldfish games to dry-run (default 20).
    #[serde(default)]
    pub games: Option<u64>,
}

// ---- tools ----------------------------------------------------------------

#[tool_router]
impl KilnServer {
    pub fn new(state: ServerState) -> Self {
        Self {
            state: Arc::new(state),
            jobs: Arc::new(Mutex::new(BTreeMap::new())),
            tool_router: Self::tool_router(),
        }
    }

    fn run_dir(&self, run_id: &str) -> PathBuf {
        if let Some(j) = self
            .jobs
            .lock()
            .ok()
            .and_then(|j| j.get(run_id).map(|j| j.dir.clone()))
        {
            return j;
        }
        self.state.workdir.join("runs").join(run_id)
    }

    #[tool(
        description = "Parse and validate a Commander decklist. Returns a deck id, the file path to use in experiment specs, legality problems, and which cards the engine can't run yet (only 'verified' cards run by default)."
    )]
    async fn kiln_deck_import(
        &self,
        Parameters(p): Parameters<DeckImportParams>,
    ) -> Result<String, String> {
        let state = Arc::clone(&self.state);
        blocking(move || {
            let path = state.workdir.join(&p.source);
            let text = if !p.source.contains('\n') && path.is_file() {
                std::fs::read_to_string(&path).map_err(|e| format!("can't read {}: {e}", path.display()))?
            } else {
                p.source.clone()
            };
            let parsed = parse_deck(&text, None);
            let report = validate_with(
                &state.db,
                &parsed,
                &ValidateOptions { format: "commander".into(), commanders: Vec::new() },
                &|c| state.lib.coverage(c.oracle_id),
            );
            let name = p
                .name
                .clone()
                .or_else(|| parsed.name.clone())
                .or_else(|| report.commanders.first().cloned())
                .unwrap_or_else(|| "deck".into());
            let id = slug(&name);
            let dir = state.workdir.join("decks");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let file = dir.join(format!("{id}.txt"));
            std::fs::write(&file, &text).map_err(|e| e.to_string())?;
            let by_level = |lvl: kiln_cards::CoverageLevel| -> Vec<String> {
                report.cards.iter().filter(|c| c.coverage == lvl).map(|c| c.name.clone()).collect()
            };
            Ok(to_text(&json!({
                "deck_id": id,
                "name": name,
                "file": file.strip_prefix(&state.workdir).unwrap_or(&file).display().to_string(),
                "legal": report.legal,
                "size": report.size,
                "commanders": report.commanders,
                "color_identity": report.color_identity,
                "violations": report.violations.iter().map(|v| v.message.clone()).collect::<Vec<_>>(),
                "warnings": report.warnings.iter().map(|v| v.message.clone()).collect::<Vec<_>>(),
                "coverage": report.coverage,
                "unsupported": by_level(kiln_cards::CoverageLevel::Unsupported),
                "parsed_not_verified": by_level(kiln_cards::CoverageLevel::Parsed),
            })))
        })
        .await?
    }

    #[tool(
        description = "Look up cards: Oracle text, types, mana cost, Commander legality, and whether the engine supports them (verified, parsed or unsupported, with reasons). Misspelled names get suggestions."
    )]
    async fn kiln_card_lookup(
        &self,
        Parameters(p): Parameters<CardLookupParams>,
    ) -> Result<String, String> {
        if p.names.len() > 50 {
            return Err(format!("at most 50 names per call; got {}", p.names.len()));
        }
        let state = Arc::clone(&self.state);
        blocking(move || {
            let cards: Vec<Value> = p
                .names
                .iter()
                .map(|n| match state.db.resolve(n, "commander") {
                    NameMatch::Exact(i) | NameMatch::Loose(i) => {
                        let c = &state.db.cards()[i];
                        let cov = state.lib.coverage(c.oracle_id);
                        json!({
                            "name": c.name,
                            "mana_cost": c.faces.first().and_then(|f| f.mana_cost.as_ref()).map(ToString::to_string),
                            "type_line": c.faces.iter().map(|f| f.type_line.to_string()).collect::<Vec<_>>().join(" // "),
                            "oracle_text": c.faces.iter().map(|f| f.oracle_text.clone()).collect::<Vec<_>>().join("\n//\n"),
                            "commander_legal": format!("{:?}", c.legality("commander")),
                            "engine_support": format!("{:?}", cov.level()).to_lowercase(),
                            "unsupported_reasons": match &cov {
                                kiln_cards::Coverage::Unsupported { reasons } =>
                                    reasons.iter().map(|r| format!("{r:?}")).collect::<Vec<_>>(),
                                _ => Vec::new(),
                            },
                        })
                    }
                    NameMatch::Ambiguous(is) => json!({
                        "query": n,
                        "error": "several cards share this name",
                        "candidates": is.iter().map(|i| state.db.cards()[*i].name.clone()).collect::<Vec<_>>(),
                    }),
                    NameMatch::NotFound { suggestions } => json!({
                        "query": n,
                        "error": "unknown card",
                        "did_you_mean": suggestions,
                    }),
                })
                .collect();
            to_text(&json!({ "cards": cards }))
        })
        .await
    }

    #[tool(
        description = "Start an experiment from a kiln-spec/1 spec (YAML or JSON). Validates it first (unknown keys get 'did you mean'; illegal decks and unverified cards are refused with the reason). Runs in the background and returns a run id at once: poll kiln_sim_status, then call kiln_sim_report. Running the same spec again resumes it."
    )]
    async fn kiln_sim_run(
        &self,
        Parameters(p): Parameters<SimRunParams>,
    ) -> Result<String, String> {
        let state = Arc::clone(&self.state);
        let resolved = kiln_protocol::load_spec(&p.spec_text).map_err(|e| e.to_string())?;
        let run_id = resolved.spec.name.clone();
        if let Ok(jobs) = self.jobs.lock()
            && jobs
                .get(&run_id)
                .is_some_and(|j| j.finished.lock().is_ok_and(|f| f.is_none()))
        {
            return Err(format!(
                "run {run_id} is already running; poll kiln_sim_status"
            ));
        }
        let st = Arc::clone(&state);
        let exp = blocking(move || Experiment::prepare(resolved, &st.workdir, &st.db, &st.lib))
            .await?
            .map_err(|e| e.to_string())?;
        let dir = state.workdir.join(
            exp.resolved
                .spec
                .outputs
                .dir
                .clone()
                .unwrap_or_else(|| format!("runs/{run_id}")),
        );
        let games_total = exp.games() * exp.configs().len() as u64;
        let warnings = exp.warnings.clone();
        let hash = exp.resolved.hash.clone();
        let threads = p.threads.unwrap_or(state.default_threads).max(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(Mutex::new(None));
        {
            let (dir, cancel, finished) = (dir.clone(), Arc::clone(&cancel), Arc::clone(&finished));
            std::thread::spawn(move || {
                let progress = |_: Progress| {};
                let outcome = experiment::run(
                    &exp,
                    &dir,
                    RunOptions {
                        threads: Some(threads),
                        cancel: &cancel,
                        on_progress: &progress,
                    },
                )
                .and_then(|_| report::write_report(&dir).map(|_| ()))
                .map_err(|e| e.to_string());
                if let Ok(mut f) = finished.lock() {
                    *f = Some(outcome);
                }
            });
        }
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.insert(
                run_id.clone(),
                Job {
                    dir: dir.clone(),
                    games_total,
                    cancel,
                    started: Instant::now(),
                    finished,
                },
            );
        }
        Ok(to_text(&json!({
            "run_id": run_id,
            "status": "running",
            "games_total": games_total,
            "spec_hash": hash,
            "dir": dir.display().to_string(),
            "warnings": warnings,
            "next": format!("kiln_sim_status run_id={run_id}"),
        })))
    }

    #[tool(
        description = "Progress of a run: games done and total, status (running, complete, cancelled, failed), elapsed time and an ETA. Pass cancel=true to stop it."
    )]
    async fn kiln_sim_status(
        &self,
        Parameters(p): Parameters<RunIdParams>,
    ) -> Result<String, String> {
        let (job_state, elapsed, job_total) = match self.jobs.lock() {
            Ok(jobs) => match jobs.get(&p.run_id) {
                Some(j) => {
                    if p.cancel {
                        j.cancel.store(true, Ordering::Relaxed);
                    }
                    let f = j.finished.lock().ok().and_then(|f| f.clone());
                    (Some(f), j.started.elapsed().as_secs_f64(), j.games_total)
                }
                None => (None, 0.0, 0),
            },
            Err(_) => (None, 0.0, 0),
        };
        let dir = self.run_dir(&p.run_id);
        let m = match experiment::read_manifest(&dir) {
            Ok(m) => m,
            // Just started: the run hasn't written its manifest yet.
            Err(_) if matches!(job_state, Some(None)) => {
                return Ok(to_text(&json!({
                    "run_id": p.run_id,
                    "status": "starting",
                    "games_done": 0,
                    "games_total": job_total,
                    "next": status_next("starting", &p.run_id, None),
                })));
            }
            Err(_) => {
                return Err(format!(
                    "no run named {:?}; start one with kiln_sim_run",
                    p.run_id
                ));
            }
        };
        let status = match &job_state {
            Some(None) if p.cancel => "cancelling".to_owned(),
            Some(None) => "running".to_owned(),
            Some(Some(Err(e))) => format!("failed: {e}"),
            _ => format!("{:?}", m.status).to_lowercase(),
        };
        let eta = (status == "running" && m.games_done > 0)
            .then(|| elapsed / m.games_done as f64 * (m.games_total - m.games_done) as f64);
        Ok(to_text(&json!({
            "run_id": p.run_id,
            "status": status,
            "games_done": m.games_done,
            "games_total": m.games_total,
            "elapsed_seconds": (elapsed * 10.0).round() / 10.0,
            "eta_seconds": eta.map(f64::round),
            "warnings": m.warnings,
            "next": status_next(&status, &p.run_id, eta),
        })))
    }

    #[tool(
        description = "A run's results. concise (default): win rates with 95% intervals and by-seat rates, median win turns, win types, keep rates, warnings, notable findings and next steps. detailed: the full report filtered by sections, with per_card paginated. markdown: a human summary."
    )]
    async fn kiln_sim_report(
        &self,
        Parameters(p): Parameters<SimReportParams>,
    ) -> Result<String, String> {
        let dir = self.run_dir(&p.run_id);
        let r = blocking(move || report::write_report(&dir))
            .await?
            .map_err(|e| e.to_string())?;
        match p.format.as_deref().unwrap_or("concise") {
            "concise" => {
                let mut c = report::concise(&r);
                c.next = c.next.iter().map(|n| mcp_hint(n, &p.run_id)).collect();
                Ok(to_text(&serde_json::to_value(c).unwrap_or_default()))
            }
            "markdown" => Ok(report::markdown(&r)),
            "detailed" => {
                let mut v = serde_json::to_value(&r).unwrap_or_default();
                let wanted: Vec<String> = if p.sections.is_empty() {
                    [
                        "win_rates",
                        "game_shape",
                        "mulligans",
                        "interaction",
                        "mana",
                        "timing",
                        "decision_stats",
                        "policies",
                    ]
                    .map(String::from)
                    .to_vec()
                } else {
                    p.sections.clone()
                };
                let (page, size) = (p.page.unwrap_or(1).max(1), p.page_size.unwrap_or(25).max(1));
                if let Some(configs) = v.get_mut("configs").and_then(Value::as_array_mut) {
                    for c in configs {
                        if let Some(obj) = c.as_object_mut() {
                            obj.retain(|k, _| {
                                matches!(k.as_str(), "config" | "games" | "capped" | "draws")
                                    || wanted.contains(k)
                            });
                            if let Some(decks) = obj
                                .get_mut("per_card")
                                .and_then(|pc| pc.get_mut("decks"))
                                .and_then(Value::as_array_mut)
                            {
                                for d in decks {
                                    if let Some(cards) =
                                        d.get_mut("cards").and_then(Value::as_array_mut)
                                    {
                                        let pages = cards.len().div_ceil(size);
                                        let slice: Vec<Value> = cards
                                            .iter()
                                            .skip((page - 1) * size)
                                            .take(size)
                                            .cloned()
                                            .collect();
                                        *cards = slice;
                                        d["page"] = json!(page);
                                        d["pages"] = json!(pages);
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(to_text(&v))
            }
            other => Err(format!(
                "format must be concise, detailed or markdown, not {other:?}"
            )),
        }
    }

    #[tool(
        description = "Compare two runs deck by deck: win rates, the difference, a p-value and whether the intervals overlap. For a card's causal effect, prefer one run with mode: ab."
    )]
    async fn kiln_sim_compare(
        &self,
        Parameters(p): Parameters<CompareParams>,
    ) -> Result<String, String> {
        let (a, b) = (self.run_dir(&p.run_a), self.run_dir(&p.run_b));
        let c = blocking(move || inspect::compare(&a, &b))
            .await?
            .map_err(|e| e.to_string())?;
        Ok(to_text(&serde_json::to_value(c).unwrap_or_default()))
    }

    #[tool(
        description = "Play one game of a run again and return what happened: each seat's deck, mulligans and fate, and the last events with card names (casts, combat damage, players losing)."
    )]
    async fn kiln_game_inspect(
        &self,
        Parameters(p): Parameters<InspectParams>,
    ) -> Result<String, String> {
        let state = Arc::clone(&self.state);
        let dir = self.run_dir(&p.run_id);
        let i = blocking(move || {
            inspect::inspect(
                &dir,
                p.game_index,
                p.config.as_deref().unwrap_or("base"),
                p.events.unwrap_or(60),
                &state.db,
                &state.lib,
            )
        })
        .await?
        .map_err(|e| e.to_string())?;
        Ok(to_text(&serde_json::to_value(i).unwrap_or_default()))
    }

    #[tool(
        description = "Rules judge: run a kiln-scenario/1 board (players' zones, a script of choices by label, expectations) and return the event log with card names, the result, and whether the expectations held (with the first mismatch)."
    )]
    async fn kiln_rules_scenario(
        &self,
        Parameters(p): Parameters<ScenarioParams>,
    ) -> Result<String, String> {
        let state = Arc::clone(&self.state);
        blocking(move || {
            let s = kiln_testkit::scenario::parse(&p.scenario, "scenario")
                .map_err(|e| e.to_string())?;
            match kiln_testkit::scenario::run(&s, &state.scenario_pool) {
                Ok(game) => Ok(to_text(&json!({
                    "expectations_held": true,
                    // The scenario format's words: `win P2`, `draw`, `in_progress`.
                    "result": match game.result() {
                        Some(kiln_engine::GameResult::Win { player, turn }) => {
                            format!("win P{} on turn {turn}", player.0 + 1)
                        }
                        Some(kiln_engine::GameResult::Draw { turn }) => format!("draw on turn {turn}"),
                        Some(kiln_engine::GameResult::Aborted { reason, turn }) => {
                            format!("aborted on turn {turn}: {reason:?}")
                        }
                        None => "in_progress".into(),
                    },
                    "events": inspect::describe_events(&game, &state.scenario_pool),
                }))),
                Err(e) => Ok(to_text(
                    &json!({ "expectations_held": false, "error": e.to_string() }),
                )),
            }
        })
        .await?
    }

    #[tool(
        description = "Check a playbook: compile errors with line and column, then a goldfish dry run reporting which rules fired (tag counts), how often it deferred to its fallback, wins, and runtime errors. Returns the playbook's file path for use in specs: { kind: playbook, playbook: <path> }."
    )]
    async fn kiln_playbook_validate(
        &self,
        Parameters(p): Parameters<PlaybookParams>,
    ) -> Result<String, String> {
        let state = Arc::clone(&self.state);
        blocking(move || {
            let dir = state.workdir.join("playbooks");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let name = p.name.as_deref().map_or_else(|| "playbook".into(), slug);
            let file = dir.join(format!("{name}.rhai"));
            std::fs::write(&file, &p.playbook_text).map_err(|e| e.to_string())?;
            let deck = state.workdir.join(&p.deck);
            let check = check_playbook(
                &deck,
                &file,
                p.games.unwrap_or(20).min(200),
                &state.db,
                &state.lib,
            )
            .map_err(|e| e.to_string())?;
            let mut v = serde_json::to_value(check).unwrap_or_default();
            v["file"] = json!(
                file.strip_prefix(&state.workdir)
                    .unwrap_or(&file)
                    .display()
                    .to_string()
            );
            Ok(to_text(&v))
        })
        .await?
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for KilnServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("kiln", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

/// Serves MCP over stdio until the client disconnects.
pub async fn serve_stdio(state: ServerState) -> anyhow::Result<()> {
    let service = KilnServer::new(state)
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}

/// Where relative paths in specs start, for callers that need it.
pub fn workdir_of(state: &ServerState) -> &Path {
    &state.workdir
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The server over a real card DB (skipped without one).
    fn server() -> Option<KilnServer> {
        let db = kiln_testkit::real::locate_db()?;
        let root = kiln_testkit::real::repo_root();
        let defs = kiln_cards::defs::load_dir(&root.join("cards/defs")).ok()?;
        let verified = kiln_testkit::real::verify(&db)
            .ok()?
            .verified
            .into_keys()
            .collect();
        let lib = Library::build(&db, &defs, &verified);
        let (pool, _) = kiln_testkit::real::real_pool(&db, &defs);
        let workdir = std::env::temp_dir().join(format!("kiln-mcp-test-{}", std::process::id()));
        std::fs::create_dir_all(&workdir).ok()?;
        Some(KilnServer::new(ServerState {
            db,
            lib,
            scenario_pool: Arc::new(pool),
            workdir,
            default_threads: 1,
        }))
    }

    #[tokio::test]
    async fn tools_answer_and_errors_are_actionable() {
        let Some(s) = server() else { return };
        let root = kiln_testkit::real::repo_root();
        let r = s
            .kiln_card_lookup(Parameters(CardLookupParams {
                names: vec!["Thassas Oracle".into()],
            }))
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(
            r.contains("Thassa's Oracle") && r.contains("verified"),
            "{r}"
        );

        let r = s
            .kiln_deck_import(Parameters(DeckImportParams {
                source: root
                    .join("tests/fixtures/decks/najeela.txt")
                    .display()
                    .to_string(),
                name: Some("Najeela".into()),
            }))
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        let v: Value = serde_json::from_str(&r).unwrap_or_default();
        assert_eq!(v["legal"], json!(true));
        assert_eq!(v["file"], json!("decks/najeela.txt"));

        let e = s
            .kiln_sim_run(Parameters(SimRunParams {
                spec_text: "spec: kiln-spec/1\nname: x\ngmaes: 3\n".into(),
                threads: None,
            }))
            .await
            .expect_err("unknown key");
        assert!(e.contains("did you mean `games`?"), "{e}");

        let e = s
            .kiln_sim_status(Parameters(RunIdParams {
                run_id: "nope".into(),
                cancel: false,
            }))
            .await
            .expect_err("no such run");
        assert!(e.contains("kiln_sim_run"), "{e}");
    }
}
