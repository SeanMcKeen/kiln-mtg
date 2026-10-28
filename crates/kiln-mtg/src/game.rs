//! `kiln game …`: replaying and inspecting single games
//! (`docs/07 Interfaces/CLI & REPL.md`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Subcommand;
use kiln_engine::Replay;
use kiln_sim::experiment::inspect::inspect;

use crate::{DbArgs, card_library, print_json};

#[derive(Debug, Subcommand)]
pub enum GameCmd {
    /// Re-run a replay file (kiln-replay/1) and check it reproduces exactly.
    Replay {
        replay: PathBuf,
        #[command(flatten)]
        db: DbArgs,
        #[arg(long)]
        json: bool,
    },
    /// Play one game of a run again and show what happened.
    Inspect {
        /// The run directory.
        dir: PathBuf,
        /// The game index.
        index: u64,
        /// The configuration (`base`, or an A/B variant).
        #[arg(long, default_value = "base")]
        config: String,
        /// How many of the last events to show (0: all).
        #[arg(long, default_value_t = 60)]
        events: usize,
        #[command(flatten)]
        db: DbArgs,
        #[arg(long)]
        json: bool,
    },
}

pub fn game(cmd: GameCmd) -> Result<ExitCode> {
    match cmd {
        GameCmd::Replay { replay, db, json } => replay_file(&replay, &db, json),
        GameCmd::Inspect {
            dir,
            index,
            config,
            events,
            db,
            json,
        } => {
            let (db, _) = db.load()?;
            let lib = card_library(&db)?;
            let i = match inspect(&dir, index, &config, events, &db, &lib) {
                Ok(i) => i,
                Err(e) => {
                    eprintln!("error: {e}");
                    return Ok(ExitCode::from(e.exit_code()));
                }
            };
            if i.diverged {
                eprintln!(
                    "warning: this game played differently than the run recorded (determinism divergence)"
                );
            }
            if json {
                print_json(&i)?;
            } else {
                let r = &i.record;
                println!("game {index} ({config}), seed {:#x}", r.seed);
                for (seat, d) in r.seats.iter().enumerate() {
                    let fate = if r.winner == Some(seat) {
                        format!("won ({})", r.win_type.clone().unwrap_or_default())
                    } else {
                        r.eliminations.iter().find(|e| e.seat == seat).map_or_else(
                            || "still in".into(),
                            |e| format!("out on turn {} ({})", e.turn, e.reason),
                        )
                    };
                    println!(
                        "  seat {}: {d} — {} mulligan(s), {fate}",
                        seat + 1,
                        r.mulligans[seat]
                    );
                }
                println!(
                    "  {:?} after {} turns, {} decisions\n",
                    r.outcome, r.turns, r.decisions
                );
                for l in &i.events {
                    println!("{l}");
                }
            }
            Ok(if i.diverged {
                ExitCode::from(5)
            } else {
                ExitCode::SUCCESS
            })
        }
    }
}

fn replay_file(path: &Path, db: &DbArgs, json: bool) -> Result<ExitCode> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let replay: Replay =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    let (db, _) = db.load()?;
    let lib = card_library(&db)?;
    let (pool, _) = lib.pool();
    match replay.run(Arc::new(pool)) {
        Ok(g) => {
            if json {
                print_json(&serde_json::json!({
                    "reproduced": true,
                    "result": g.result(),
                    "turns": g.turn().number,
                    "choices": replay.choices.len(),
                }))?;
            } else {
                println!(
                    "reproduced: {} choices, turn {}, result {:?}",
                    replay.choices.len(),
                    g.turn().number,
                    g.result()
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("error: the replay diverged: {e}");
            Ok(ExitCode::from(5))
        }
    }
}
