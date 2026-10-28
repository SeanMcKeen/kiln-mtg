//! `kiln judge`: the rules-judge REPL (`docs/07 Interfaces/Judge Tool UX.md`, M10 plan L3).

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use clap::Args;
use kiln_engine::CardPool;
use kiln_testkit::judge::Judge;

#[derive(Debug, Args)]
pub struct JudgeArgs {
    /// A kiln-scenario/1 file: its script sets the position up.
    pub scenario: PathBuf,
    /// Run these commands instead of reading them from stdin (for agents
    /// and scripts), e.g. `--command "why-not Cast Thassa's Oracle" --command 0`.
    #[arg(long = "command", short = 'c')]
    pub commands: Vec<String>,
    /// Card DB file (default: the `current` DB in the cache directory).
    #[arg(long)]
    pub db: Option<PathBuf>,
    #[arg(long, env = "KILN_CACHE_DIR", default_value = kiln_data::DEFAULT_CACHE_DIR)]
    pub cache: PathBuf,
}

/// The real-card pool when the card DB is available, else the synthetic test cards.
fn pool(args: &JudgeArgs) -> Result<Arc<CardPool>> {
    let Ok(path) = kiln_data::CardDb::locate(args.db.as_deref(), &args.cache) else {
        return Ok(Arc::new(CardPool::m3_test_set()));
    };
    let db = kiln_data::CardDb::load(&path)?;
    let defs = kiln_cards::defs::load_dir(&kiln_testkit::real::repo_root().join("cards/defs"))?;
    Ok(Arc::new(kiln_testkit::real::real_pool(&db, &defs).0))
}

pub fn judge(args: &JudgeArgs) -> Result<ExitCode> {
    let s = kiln_testkit::scenario::load(&args.scenario).map_err(|e| anyhow!("{e}"))?;
    let pool = pool(args)?;
    let mut j = Judge::load(&s, &pool).map_err(|e| anyhow!("set-up: {e}"))?;
    println!("{}", s.name);
    println!("{}", j.prompt());
    let mut run = |line: &str| match j.command(line) {
        Ok(out) => println!("{out}"),
        Err(e) => println!("error: {e}"),
    };
    if !args.commands.is_empty() {
        for c in &args.commands {
            println!("> {c}");
            run(c);
        }
        return Ok(ExitCode::SUCCESS);
    }
    let stdin = std::io::stdin();
    loop {
        print!("> ");
        std::io::stdout().flush()?;
        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim();
        if matches!(line, "quit" | "exit" | "q") {
            break;
        }
        if line == "help" {
            println!(
                "a number or label chooses; options; why-not [P2] <label or card>; why <event #>; events [n]; view; branch; back; result; quit"
            );
            continue;
        }
        run(line);
    }
    Ok(ExitCode::SUCCESS)
}
