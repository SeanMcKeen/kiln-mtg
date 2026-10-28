//! `kiln mcp serve`: the MCP server for AI agents (ADR-0008, M8 plan P6).

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Result;
use clap::Subcommand;

use crate::{DbArgs, card_library};

#[derive(Debug, Subcommand)]
pub enum McpCmd {
    /// Serve the kiln MCP tools over stdio (for Claude Code, Claude Desktop
    /// and other MCP clients).
    Serve {
        /// Runs, imported decks and playbooks go under here; relative paths in
        /// specs start here.
        #[arg(long, default_value = ".")]
        workdir: PathBuf,
        /// Worker threads per run unless a call asks for more.
        #[arg(long, default_value_t = 2)]
        threads: usize,
        #[command(flatten)]
        db: DbArgs,
    },
}

pub fn mcp(cmd: McpCmd) -> Result<ExitCode> {
    let McpCmd::Serve {
        workdir,
        threads,
        db,
    } = cmd;
    // stdout carries the protocol: progress goes to stderr.
    eprintln!("kiln mcp: loading the card library…");
    let (db, _) = db.load()?;
    let lib = card_library(&db)?;
    let defs = kiln_cards::defs::load_dir(&kiln_testkit::real::repo_root().join("cards/defs"))?;
    let (scenario_pool, _) = kiln_testkit::real::real_pool(&db, &defs);
    let state = kiln_mcp::ServerState {
        db,
        lib,
        scenario_pool: Arc::new(scenario_pool),
        workdir: std::fs::canonicalize(&workdir).unwrap_or(workdir),
        default_threads: threads,
    };
    eprintln!("kiln mcp: serving on stdio");
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?
        .block_on(kiln_mcp::serve_stdio(state))?;
    Ok(ExitCode::SUCCESS)
}
