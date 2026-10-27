//! The run directory (M7 plan S8): the resolved spec, a manifest, and
//! `games.jsonl`, appended as games finish so a run can resume.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use kiln_protocol::spec::ReplayKeep;
use kiln_protocol::{GameRecord, Outcome, RunManifest, RunStatus};
use rayon::prelude::*;

use super::{Experiment, ExperimentError, Played};

/// Games played between checkpoints.
const CHUNK: usize = 256;

pub const MANIFEST: &str = "run.json";
pub const GAMES: &str = "games.jsonl";
pub const SPEC: &str = "spec.resolved.yaml";
pub const PROGRESS: &str = "progress.jsonl";

/// How far a run has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub done: u64,
    pub total: u64,
}

#[derive(Clone, Copy)]
pub struct RunOptions<'a> {
    /// Worker threads; `None` = all cores.
    pub threads: Option<usize>,
    /// Set to stop after the games in progress (cooperative).
    pub cancel: &'a AtomicBool,
    pub on_progress: &'a (dyn Fn(Progress) + Sync),
}

impl std::fmt::Debug for RunOptions<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunOptions")
            .field("threads", &self.threads)
            .finish_non_exhaustive()
    }
}

fn io(path: &Path, e: impl std::fmt::Display) -> ExperimentError {
    ExperimentError::Run(format!("{}: {e}", path.display()))
}

/// Reads a run's game records, sorted by (configuration, index).
pub fn read_records(dir: &Path) -> Result<Vec<GameRecord>, ExperimentError> {
    let path = dir.join(GAMES);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io(&path, e)),
    };
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        // A line cut short by a crash is dropped; that game is played again.
        match serde_json::from_str::<GameRecord>(line) {
            Ok(r) => out.push(r),
            Err(_) if i + 1 == text.lines().count() => {}
            Err(e) => return Err(io(&path, format!("line {}: {e}", i + 1))),
        }
    }
    out.sort_by(|a, b| (&a.config, a.index).cmp(&(&b.config, b.index)));
    out.dedup_by(|a, b| a.config == b.config && a.index == b.index);
    Ok(out)
}

pub fn read_manifest(dir: &Path) -> Result<RunManifest, ExperimentError> {
    let path = dir.join(MANIFEST);
    let text = std::fs::read_to_string(&path).map_err(|e| io(&path, e))?;
    serde_json::from_str(&text).map_err(|e| io(&path, e))
}

fn write_manifest(dir: &Path, m: &RunManifest) -> Result<(), ExperimentError> {
    let path = dir.join(MANIFEST);
    let text = serde_json::to_string_pretty(m).unwrap_or_default();
    std::fs::write(&path, text + "\n").map_err(|e| io(&path, e))
}

/// Plays every game not yet in `dir`'s `games.jsonl`, checkpointing as it
/// goes. Starting again with the same spec resumes; another spec is refused.
pub fn run(
    exp: &Experiment,
    dir: &Path,
    opts: RunOptions<'_>,
) -> Result<RunManifest, ExperimentError> {
    std::fs::create_dir_all(dir).map_err(|e| io(dir, e))?;
    let spec = &exp.resolved.spec;
    if let Ok(old) = read_manifest(dir)
        && old.spec_hash != exp.resolved.hash
    {
        return Err(ExperimentError::Run(format!(
            "{} holds a run of a different spec (hash {}, now {}); use another outputs.dir",
            dir.display(),
            old.spec_hash,
            exp.resolved.hash
        )));
    }
    let spec_path = dir.join(SPEC);
    // Through a JSON value, so enums are plain maps that read back.
    let value = serde_json::to_value(spec).map_err(|e| io(&spec_path, e))?;
    let yaml = yaml_serde::to_string(&value).map_err(|e| io(&spec_path, e))?;
    std::fs::write(&spec_path, yaml).map_err(|e| io(&spec_path, e))?;

    let done: BTreeSet<(String, u64)> = read_records(dir)?
        .into_iter()
        .map(|r| (r.config, r.index))
        .collect();
    let games = exp.games();
    let configs = exp.configs();
    let total = games * configs.len() as u64;
    let todo: Vec<(usize, u64)> = (0..games)
        .flat_map(|i| (0..configs.len()).map(move |c| (c, i)))
        .filter(|(c, i)| !done.contains(&(configs[*c].name.clone(), *i)))
        .collect();
    let mut manifest = RunManifest {
        run: spec.name.clone(),
        spec_hash: exp.resolved.hash.clone(),
        card_db: exp.card_db.clone(),
        engine: env!("CARGO_PKG_VERSION").into(),
        games_total: total,
        games_done: done.len() as u64,
        status: RunStatus::Running,
        warnings: exp.warnings.clone(),
    };
    write_manifest(dir, &manifest)?;

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(opts.threads.unwrap_or(0))
        .build()
        .map_err(|e| ExperimentError::Run(e.to_string()))?;
    let games_path = dir.join(GAMES);
    let mut games_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&games_path)
        .map_err(|e| io(&games_path, e))?;
    let progress_path = dir.join(PROGRESS);
    let mut progress_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&progress_path)
        .map_err(|e| io(&progress_path, e))?;
    (opts.on_progress)(Progress {
        done: manifest.games_done,
        total,
    });
    for chunk in todo.chunks(CHUNK) {
        let played: Vec<Option<Played>> = pool.install(|| {
            chunk
                .par_iter()
                .map(|(c, i)| exp.play(&configs[*c], *i, opts.cancel, false))
                .collect()
        });
        let mut lines = String::new();
        for p in played.iter().flatten() {
            lines.push_str(&serde_json::to_string(&p.record).unwrap_or_default());
            lines.push('\n');
            manifest.games_done += 1;
            if let Some(events) = &p.events {
                write_events(dir, &p.record, events)?;
            }
        }
        games_file
            .write_all(lines.as_bytes())
            .and_then(|()| games_file.flush())
            .map_err(|e| io(&games_path, e))?;
        writeln!(
            progress_file,
            "{{\"done\":{},\"total\":{total}}}",
            manifest.games_done
        )
        .map_err(|e| io(&progress_path, e))?;
        (opts.on_progress)(Progress {
            done: manifest.games_done,
            total,
        });
        if played.iter().any(Option::is_none) || opts.cancel.load(Ordering::Relaxed) {
            manifest.status = RunStatus::Cancelled;
            write_manifest(dir, &manifest)?;
            return Ok(manifest);
        }
    }
    write_replays(exp, dir, &read_records(dir)?)?;
    manifest.status = RunStatus::Complete;
    write_manifest(dir, &manifest)?;
    Ok(manifest)
}

fn game_file(r: &GameRecord) -> String {
    format!("{}-{:06}", r.config, r.index)
}

/// `metrics.raw_events`: one JSON event per line.
fn write_events(
    dir: &Path,
    r: &GameRecord,
    events: &[kiln_engine::Event],
) -> Result<(), ExperimentError> {
    let events_dir = dir.join("events");
    std::fs::create_dir_all(&events_dir).map_err(|e| io(&events_dir, e))?;
    let path = events_dir.join(format!("{}.jsonl", game_file(r)));
    let mut text = String::new();
    for e in events {
        text.push_str(&serde_json::to_string(e).unwrap_or_default());
        text.push('\n');
    }
    std::fs::write(&path, text).map_err(|e| io(&path, e))
}

/// The games whose replays to keep (Metrics Catalog `replays`).
pub fn replay_selection(keep: ReplayKeep, max: u64, records: &[GameRecord]) -> Vec<(String, u64)> {
    let max = usize::try_from(max).unwrap_or(usize::MAX);
    let key = |r: &GameRecord| (r.config.clone(), r.index);
    let mut out: Vec<(String, u64)> = match keep {
        ReplayKeep::None => Vec::new(),
        ReplayKeep::All => records.iter().map(key).collect(),
        ReplayKeep::Sample(n) => records
            .iter()
            .filter(|r| r.index % n == 0)
            .take(max)
            .map(key)
            .collect(),
        ReplayKeep::Interesting => {
            let quarter = max.div_ceil(4).max(1);
            let played: Vec<&GameRecord> = records
                .iter()
                .filter(|r| r.outcome != Outcome::NotPlayed)
                .collect();
            let mut wins: Vec<&&GameRecord> = played
                .iter()
                .filter(|r| r.outcome == Outcome::Win)
                .collect();
            wins.sort_by_key(|r| (r.turns, r.index));
            let mut longest: Vec<&&GameRecord> = played.iter().collect();
            longest.sort_by_key(|r| (std::cmp::Reverse(r.turns), r.index));
            // Upsets: wins by the deck that won least often.
            let mut by_deck: std::collections::BTreeMap<&str, u64> = Default::default();
            for r in &wins {
                if let Some(w) = r.winner {
                    *by_deck.entry(r.seats[w].as_str()).or_default() += 1;
                }
            }
            let weakest = by_deck.iter().min_by_key(|(_, n)| **n).map(|(d, _)| *d);
            let upsets = wins
                .iter()
                .filter(|r| r.winner.map(|w| r.seats[w].as_str()) == weakest);
            let capped = played.iter().filter(|r| r.outcome == Outcome::Capped);
            let mut picked: Vec<(String, u64)> = Vec::new();
            picked.extend(wins.iter().take(quarter).map(|r| key(r)));
            picked.extend(upsets.take(quarter).map(|r| key(r)));
            picked.extend(capped.take(quarter).map(|r| key(r)));
            picked.extend(longest.iter().take(quarter).map(|r| key(r)));
            picked.sort();
            picked.dedup();
            picked.truncate(max);
            picked
        }
    };
    out.sort();
    out
}

/// Plays the selected games again with a recorder and writes their replays
/// (`kiln-replay/1`). A game that doesn't reproduce its record is a
/// determinism failure (exit code 5).
fn write_replays(
    exp: &Experiment,
    dir: &Path,
    records: &[GameRecord],
) -> Result<(), ExperimentError> {
    let spec = &exp.resolved.spec;
    let keep = ReplayKeep::parse(&spec.outputs.replays.keep).unwrap_or(ReplayKeep::None);
    let chosen = replay_selection(keep, spec.outputs.replays.max, records);
    if chosen.is_empty() {
        return Ok(());
    }
    let replays_dir = dir.join("replays");
    std::fs::create_dir_all(&replays_dir).map_err(|e| io(&replays_dir, e))?;
    let never = AtomicBool::new(false);
    for (config, index) in chosen {
        let Some(c) = exp.configs().iter().find(|c| c.name == config) else {
            continue;
        };
        let Some(played) = exp.play(c, index, &never, true) else {
            continue;
        };
        let Some(stored) = records
            .iter()
            .find(|r| r.config == config && r.index == index)
        else {
            continue;
        };
        if played.record.outcome != stored.outcome
            || played.record.winner != stored.winner
            || played.record.turns != stored.turns
            || played.record.decisions != stored.decisions
        {
            return Err(ExperimentError::Divergence(format!(
                "game {config}/{index} played differently the second time"
            )));
        }
        let path = replays_dir.join(format!("{}.json", game_file(stored)));
        let text = serde_json::to_string(&played.replay).unwrap_or_default();
        std::fs::write(&path, text).map_err(|e| io(&path, e))?;
    }
    Ok(())
}
