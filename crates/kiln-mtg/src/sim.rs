//! `kiln sim …`: running experiments and reading their reports
//! (`docs/07 Interfaces/CLI & REPL.md`, M7 plan S14).

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use anyhow::Result;
use clap::Subcommand;
use kiln_sim::experiment::{self, Experiment, ExperimentError, Progress, RunOptions, report};

use crate::{DbArgs, card_library, print_json};

#[derive(Debug, Subcommand)]
pub enum SimCmd {
    /// Run an experiment spec (kiln-spec/1). Running it again resumes.
    Run {
        spec: PathBuf,
        /// Worker threads (default: all cores).
        #[arg(long)]
        threads: Option<usize>,
        /// Run directory (default: the spec's `outputs.dir`, else `runs/<name>`).
        #[arg(long)]
        dir: Option<PathBuf>,
        #[command(flatten)]
        db: DbArgs,
        /// Print the concise report as JSON.
        #[arg(long)]
        json: bool,
    },
    /// A run's progress and status.
    Status {
        dir: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Compare two runs' win rates deck by deck (base configurations).
    Compare {
        a: PathBuf,
        b: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Rebuild and print a run's report.
    Report {
        dir: PathBuf,
        /// concise | detailed | markdown
        #[arg(long, default_value = "concise")]
        format: String,
        #[arg(long)]
        json: bool,
    },
}

/// Exit with the CLI's code for an experiment error (2 input, 3 coverage, 4 run).
fn fail(e: &ExperimentError) -> ExitCode {
    eprintln!("error: {e}");
    ExitCode::from(e.exit_code())
}

pub fn sim(cmd: SimCmd) -> Result<ExitCode> {
    match cmd {
        SimCmd::Run {
            spec,
            threads,
            dir,
            db,
            json,
        } => run(&spec, threads, dir, &db, json),
        SimCmd::Status { dir, json } => match experiment::read_manifest(&dir) {
            Ok(m) => {
                if json {
                    print_json(&m)?;
                } else {
                    println!(
                        "{}: {:?}, {} of {} games · spec {} · card DB {}",
                        m.run, m.status, m.games_done, m.games_total, m.spec_hash, m.card_db
                    );
                }
                Ok(ExitCode::SUCCESS)
            }
            Err(e) => Ok(fail(&e)),
        },
        SimCmd::Compare { a, b, json } => compare(&a, &b, json),
        SimCmd::Report { dir, format, json } => match experiment::write_report(&dir) {
            Ok(r) => {
                print_report(&r, &format, json)?;
                Ok(ExitCode::SUCCESS)
            }
            Err(e) => Ok(fail(&e)),
        },
    }
}

/// One deck's win rate in two runs.
#[derive(Debug, serde::Serialize)]
struct DeckDiff {
    deck: String,
    a: kiln_protocol::report::Rate,
    b: kiln_protocol::report::Rate,
    /// b − a.
    difference: f64,
    /// Two-proportion z-test.
    p_value: f64,
    intervals_overlap: bool,
}

fn compare(a: &Path, b: &Path, json: bool) -> Result<ExitCode> {
    let (ra, rb) = match (experiment::write_report(a), experiment::write_report(b)) {
        (Ok(x), Ok(y)) => (x, y),
        (Err(e), _) | (_, Err(e)) => return Ok(fail(&e)),
    };
    let rates = |r: &kiln_protocol::Report| {
        r.configs
            .first()
            .and_then(|c| c.win_rates.clone())
            .map(|w| w.decks)
            .unwrap_or_default()
    };
    let (da, db) = (rates(&ra), rates(&rb));
    let diffs: Vec<DeckDiff> = da
        .iter()
        .filter_map(|x| {
            let y = db.iter().find(|y| y.deck == x.deck)?;
            Some(DeckDiff {
                deck: x.deck.clone(),
                difference: report::r4_pub(y.win.rate - x.win.rate),
                p_value: report::r4_pub(experiment::stats::two_proportion_p(
                    x.win.k, x.win.n, y.win.k, y.win.n,
                )),
                intervals_overlap: x.win.ci95[0] <= y.win.ci95[1] && y.win.ci95[0] <= x.win.ci95[1],
                a: x.win.clone(),
                b: y.win.clone(),
            })
        })
        .collect();
    let paired = ra.versions.spec_hash != rb.versions.spec_hash;
    if json {
        print_json(&serde_json::json!({
            "a": ra.run_id, "b": rb.run_id,
            "same_card_db": ra.versions.card_db == rb.versions.card_db,
            "decks": diffs,
            "note": "independent runs: for a causal card comparison use mode: ab (paired seeds)",
        }))?;
    } else {
        println!("{} → {}", ra.run_id, rb.run_id);
        println!("| Deck | A | B | B − A | p | intervals overlap |\n|---|---|---|---|---|---|");
        for d in &diffs {
            println!(
                "| {} | {:.1}% | {:.1}% | {:+.1} pp | {:.3} | {} |",
                d.deck,
                d.a.rate * 100.0,
                d.b.rate * 100.0,
                d.difference * 100.0,
                d.p_value,
                if d.intervals_overlap { "yes" } else { "no" }
            );
        }
        if !paired {
            println!("(same spec in both: differences are run-to-run noise only if seeds differ)");
        }
        if ra.versions.card_db != rb.versions.card_db {
            println!("warning: the runs used different card DBs");
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn print_report(r: &kiln_protocol::Report, format: &str, json: bool) -> Result<()> {
    match (format, json) {
        ("detailed", _) => print_json(r),
        ("markdown", false) => {
            print!("{}", report::markdown(r));
            Ok(())
        }
        ("concise", _) | (_, true) => print_json(&report::concise(r)),
        (other, false) => {
            anyhow::bail!("--format must be concise, detailed or markdown, not {other}")
        }
    }
}

fn run(
    spec_path: &Path,
    threads: Option<usize>,
    dir: Option<PathBuf>,
    db: &DbArgs,
    json: bool,
) -> Result<ExitCode> {
    let text = std::fs::read_to_string(spec_path)
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", spec_path.display()))?;
    let resolved = match kiln_protocol::load_spec(&text) {
        Ok(r) => r,
        Err(e) => return Ok(fail(&e.into())),
    };
    let (db, _) = db.load()?;
    if !json {
        eprintln!("loading the card library (verifying card definitions)…");
    }
    let lib = card_library(&db)?;
    let spec_dir = spec_path.parent().unwrap_or(Path::new("."));
    let exp = match Experiment::prepare(resolved, spec_dir, &db, &lib) {
        Ok(x) => x,
        Err(e) => return Ok(fail(&e)),
    };
    let dir = dir.unwrap_or_else(|| {
        PathBuf::from(
            exp.resolved
                .spec
                .outputs
                .dir
                .clone()
                .unwrap_or_else(|| format!("runs/{}", exp.resolved.spec.name)),
        )
    });
    for w in &exp.warnings {
        eprintln!("warning: {w}");
    }
    let cancel = AtomicBool::new(false);
    let started = Instant::now();
    let quiet = json;
    let progress = move |p: Progress| {
        if !quiet {
            eprintln!(
                "  {} / {} games ({:.0}s)",
                p.done,
                p.total,
                started.elapsed().as_secs_f64()
            );
        }
    };
    let manifest = match experiment::run(
        &exp,
        &dir,
        RunOptions {
            threads,
            cancel: &cancel,
            on_progress: &progress,
        },
    ) {
        Ok(m) => m,
        Err(e) => return Ok(fail(&e)),
    };
    let report = match experiment::write_report(&dir) {
        Ok(r) => r,
        Err(e) => return Ok(fail(&e)),
    };
    if json {
        print_json(&report::concise(&report))?;
    } else {
        print!("{}", report::markdown(&report));
        eprintln!(
            "{:?}: {} games in {:.1}s → {}",
            manifest.status,
            manifest.games_done,
            started.elapsed().as_secs_f64(),
            dir.display()
        );
    }
    Ok(ExitCode::SUCCESS)
}
