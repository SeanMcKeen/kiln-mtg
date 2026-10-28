//! `kiln playbook …`: checking AI-written playbooks (M8 plan P5).

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::Subcommand;
use kiln_sim::experiment::check_playbook;

use crate::{DbArgs, card_library, print_json};

#[derive(Debug, Subcommand)]
pub enum PlaybookCmd {
    /// Compile a playbook, then play goldfish games with it: rules fired,
    /// deferrals and runtime errors. Exit 2 on a compile error.
    Check {
        /// The deck it pilots (a decklist file).
        deck: PathBuf,
        /// The playbook (a Rhai script).
        playbook: PathBuf,
        /// Goldfish games to play.
        #[arg(long, default_value_t = 50)]
        games: u64,
        #[command(flatten)]
        db: DbArgs,
        #[arg(long)]
        json: bool,
    },
}

pub fn playbook(cmd: PlaybookCmd) -> Result<ExitCode> {
    let PlaybookCmd::Check {
        deck,
        playbook,
        games,
        db,
        json,
    } = cmd;
    let (db, _) = db.load()?;
    let lib = card_library(&db)?;
    let check = match check_playbook(&deck, &playbook, games, &db, &lib) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(ExitCode::from(e.exit_code()));
        }
    };
    if json {
        print_json(&check)?;
    } else if let Some(e) = &check.compile_error {
        println!("compile error: {e}");
    } else {
        println!("{}: hooks {}", check.playbook, check.hooks.join(", "));
        println!(
            "{} goldfish games: {} wins{}; the playbook answered {:.1}% of decisions",
            check.games,
            check.wins,
            check
                .median_win_turn
                .map_or_else(String::new, |t| format!(", median win on its turn {t}")),
            check.playbook_share * 100.0
        );
        for (k, n) in &check.counters {
            println!("  {k}: {n}");
        }
        if check.errors.is_empty() {
            println!("no runtime errors");
        } else {
            println!("runtime errors (each deferred to the fallback):");
            for e in &check.errors {
                println!("  {e}");
            }
        }
    }
    Ok(if check.compile_error.is_some() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    })
}
