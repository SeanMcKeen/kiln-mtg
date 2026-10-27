//! The run directory (M7 plan S8): the resolved spec, a manifest, and
//! `games.jsonl`, appended as games finish so a run can resume.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use kiln_protocol::{GameRecord, RunManifest, RunStatus};
use rayon::prelude::*;

use super::{Experiment, ExperimentError};

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
        let records: Vec<Option<GameRecord>> = pool.install(|| {
            chunk
                .par_iter()
                .map(|(c, i)| exp.play_game(&configs[*c], *i, opts.cancel))
                .collect()
        });
        let mut lines = String::new();
        for r in records.iter().flatten() {
            lines.push_str(&serde_json::to_string(r).unwrap_or_default());
            lines.push('\n');
            manifest.games_done += 1;
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
        if records.iter().any(Option::is_none) || opts.cancel.load(Ordering::Relaxed) {
            manifest.status = RunStatus::Cancelled;
            write_manifest(dir, &manifest)?;
            return Ok(manifest);
        }
    }
    manifest.status = RunStatus::Complete;
    write_manifest(dir, &manifest)?;
    Ok(manifest)
}
