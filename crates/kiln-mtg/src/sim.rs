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
        SimCmd::Report { dir, format, json } => match experiment::write_report(&dir) {
            Ok(r) => {
                print_report(&r, &format, json)?;
                Ok(ExitCode::SUCCESS)
            }
            Err(e) => Ok(fail(&e)),
        },
    }
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
