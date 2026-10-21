//! `mtg`: command-line interface (`docs/07 Interfaces/CLI & REPL.md`).
//!
//! Human-readable output by default; `--json` prints machine-readable JSON on
//! stdout. Exit codes: 0 = ok/legal, 1 = check failed (illegal deck, invalid
//! citations), 2 = usage or data error.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use mtg_cards::CoverageLevel;
use mtg_data::{BuildInput, CardDb, DEFAULT_CACHE_DIR};
use mtg_sim::deck::{DeckReport, ValidateOptions, parse_deck, validate};
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(name = "mtg", version, about = "MTG rules engine and simulation lab", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Card data: fetch, build and inspect the card database.
    #[command(subcommand)]
    Data(DataCmd),
    /// Decklists: validate format legality and card coverage.
    #[command(subcommand)]
    Deck(DeckCmd),
    /// Developer tools.
    #[command(subcommand)]
    Dev(DevCmd),
}

#[derive(Debug, Args)]
struct DbArgs {
    /// Card DB file (default: the `current` DB in the cache directory).
    #[arg(long)]
    db: Option<PathBuf>,
    /// Cache directory holding downloads and built DBs.
    #[arg(long, env = "MTG_CACHE_DIR", default_value = DEFAULT_CACHE_DIR)]
    cache: PathBuf,
}

impl DbArgs {
    fn load(&self) -> Result<(CardDb, f64)> {
        let path = CardDb::locate(self.db.as_deref(), &self.cache)?;
        let t = Instant::now();
        let db = CardDb::load(&path)?;
        Ok((db, t.elapsed().as_secs_f64() * 1000.0))
    }
}

#[derive(Debug, Subcommand)]
enum DataCmd {
    /// Download Scryfall bulk data (oracle_cards, and rulings with --rulings). Cached ≥ 24 h.
    #[cfg(feature = "fetch")]
    Fetch {
        #[arg(long, env = "MTG_CACHE_DIR", default_value = DEFAULT_CACHE_DIR)]
        cache: PathBuf,
        /// Download even if the cached copy is younger than 24 hours.
        #[arg(long)]
        force: bool,
        /// Also download the rulings bulk file.
        #[arg(long)]
        rulings: bool,
        #[arg(long)]
        json: bool,
    },
    /// Build the binary card DB from a bulk file (offline).
    Build {
        /// Scryfall oracle_cards file (JSONL or JSON, optionally .gz). Default: `<cache>/oracle_cards.jsonl`.
        #[arg(long)]
        input: Option<PathBuf>,
        /// Scryfall rulings file to attach ruling counts. Default: `<cache>/rulings.json*` if present.
        #[arg(long)]
        rulings: Option<PathBuf>,
        /// Source date (YYYY-MM-DD). Default: from the download's metadata, else `unknown`.
        #[arg(long)]
        source_date: Option<String>,
        #[arg(long, env = "MTG_CACHE_DIR", default_value = DEFAULT_CACHE_DIR)]
        cache: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Show the card DB's version, size and load time.
    Info {
        #[command(flatten)]
        db: DbArgs,
        #[arg(long)]
        json: bool,
    },
    /// Coverage summary for a format's legal cards, or for a deck.
    Coverage {
        #[arg(long, default_value = "commander")]
        format: String,
        /// Report coverage for this decklist instead of the whole format.
        #[arg(long)]
        deck: Option<PathBuf>,
        #[command(flatten)]
        db: DbArgs,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum DeckCmd {
    /// Validate a decklist: format legality plus per-card coverage.
    Check {
        /// Decklist file (text export or CSV).
        file: PathBuf,
        #[arg(long, default_value = "commander")]
        format: String,
        /// Commander name(s), if the list doesn't mark them. Repeat for partners.
        #[arg(long = "commander")]
        commanders: Vec<String>,
        /// List every card with its coverage in human output.
        #[arg(long)]
        cards: bool,
        #[command(flatten)]
        db: DbArgs,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum DevCmd {
    /// Check every `CR x.y` citation in code and docs against the Comprehensive Rules.
    CrCheck {
        /// Normalized CR text (see CLAUDE.md for how to download it).
        #[arg(long, env = "MTG_CR_PATH", default_value = "docs/_external/cr-lf.txt")]
        cr: PathBuf,
        /// Files or directories to scan.
        #[arg(default_values = ["docs", "crates"])]
        paths: Vec<PathBuf>,
        /// Fail (exit 2) instead of skipping when the CR file is missing.
        #[arg(long)]
        require: bool,
        #[arg(long)]
        json: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        #[cfg(feature = "fetch")]
        Command::Data(DataCmd::Fetch {
            cache,
            force,
            rulings,
            json,
        }) => data_fetch(&cache, force, rulings, json),
        Command::Data(DataCmd::Build {
            input,
            rulings,
            source_date,
            cache,
            json,
        }) => data_build(input, rulings, source_date, &cache, json),
        Command::Data(DataCmd::Info { db, json }) => data_info(&db, json),
        Command::Data(DataCmd::Coverage {
            format,
            deck,
            db,
            json,
        }) => data_coverage(&format, deck.as_deref(), &db, json),
        Command::Deck(DeckCmd::Check {
            file,
            format,
            commanders,
            cards,
            db,
            json,
        }) => deck_check(&file, format, commanders, cards, &db, json),
        Command::Dev(DevCmd::CrCheck {
            cr,
            paths,
            require,
            json,
        }) => cr_check(&cr, &paths, require, json),
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[cfg(feature = "fetch")]
fn data_fetch(cache: &Path, force: bool, rulings: bool, json: bool) -> Result<ExitCode> {
    use mtg_data::fetch::{Fetched, fetch_bulk};
    let mut kinds = vec!["oracle_cards"];
    if rulings {
        kinds.push("rulings");
    }
    let mut out = Vec::new();
    for kind in kinds {
        let r = fetch_bulk(kind, cache, force)?;
        let (status, path, meta, age) = match r {
            Fetched::Downloaded { path, meta } => ("downloaded", path, meta, None),
            Fetched::CachedFresh {
                path,
                meta,
                age_secs,
            } => ("cached", path, meta, Some(age_secs)),
        };
        if !json {
            match age {
                Some(a) => println!(
                    "{kind}: cached copy is {}h old (< 24h), not re-downloading; use --force to override\n  {}",
                    a / 3600,
                    path.display()
                ),
                None => println!(
                    "{kind}: downloaded {} bytes (Scryfall updated {})\n  {}",
                    meta.size,
                    meta.updated_at,
                    path.display()
                ),
            }
        }
        out.push(serde_json::json!({ "kind": kind, "status": status, "path": path, "meta": meta }));
    }
    if json {
        print_json(&out)?;
    } else {
        println!("next: mtg data build");
    }
    Ok(ExitCode::SUCCESS)
}

fn first_existing(cache: &Path, stem: &str) -> Option<PathBuf> {
    ["jsonl", "json", "jsonl.gz", "json.gz"]
        .iter()
        .map(|ext| cache.join(format!("{stem}.{ext}")))
        .find(|p| p.exists())
}

fn data_build(
    input: Option<PathBuf>,
    rulings: Option<PathBuf>,
    source_date: Option<String>,
    cache: &Path,
    json: bool,
) -> Result<ExitCode> {
    let Some(input) = input.or_else(|| first_existing(cache, "oracle_cards")) else {
        bail!(
            "no oracle_cards file in {}; run `mtg data fetch` or pass --input PATH",
            cache.display()
        );
    };
    let rulings = rulings.or_else(|| first_existing(cache, "rulings"));
    #[cfg(feature = "fetch")]
    let meta_date = mtg_data::fetch::FetchMeta::read(&input).map(|m| m.source_date());
    #[cfg(not(feature = "fetch"))]
    let meta_date: Option<String> = None;
    let source_date = source_date
        .or(meta_date)
        .unwrap_or_else(|| "unknown".into());
    let t = Instant::now();
    let (db, report) = mtg_data::build(&BuildInput {
        oracle_cards: &input,
        rulings: rulings.as_deref(),
        source: "scryfall/oracle_cards".into(),
        source_date,
    })
    .with_context(|| format!("building card DB from {}", input.display()))?;
    let build_ms = t.elapsed().as_millis();
    let path = db.write_to_dir(cache)?;
    let stem = path.with_extension("");
    let report_json = stem.with_file_name(format!(
        "{}-report.json",
        stem.file_name()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default()
    ));
    let report_md = report_json.with_extension("md");
    std::fs::write(&report_json, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("writing {}", report_json.display()))?;
    std::fs::write(&report_md, report.to_markdown())
        .with_context(|| format!("writing {}", report_md.display()))?;
    if json {
        print_json(
            &serde_json::json!({ "db": path, "report": report_json, "build_ms": build_ms, "summary": {
                "db_id": report.db_id, "cards": report.cards, "deck_cards": report.deck_cards,
                "unknown_layouts": report.unknown_layouts, "cards_with_issues": report.cards_with_issues,
                "color_identity_mismatches": report.color_identity_mismatches.len(), "duplicate_names": report.duplicate_names.len(),
            }}),
        )?;
    } else {
        println!("Built card DB {} in {build_ms} ms", report.db_id);
        println!(
            "  {} cards ({} deck-eligible), {} with data issues",
            report.cards, report.deck_cards, report.cards_with_issues
        );
        println!("  unknown layouts: {:?}", report.unknown_layouts);
        println!(
            "  color identity mismatches vs source: {}",
            report.color_identity_mismatches.len()
        );
        println!(
            "  duplicate deck-card names: {}",
            report.duplicate_names.len()
        );
        println!("  db:     {}", path.display());
        println!("  report: {}", report_md.display());
    }
    Ok(ExitCode::SUCCESS)
}

fn data_info(args: &DbArgs, json: bool) -> Result<ExitCode> {
    let (db, load_ms) = args.load()?;
    let deck_cards = db
        .cards()
        .iter()
        .filter(|c| c.layout.is_deck_card())
        .count();
    if json {
        print_json(
            &serde_json::json!({ "db_id": db.id(), "version": db.version, "content_sha256": db.content_sha256, "cards": db.len(), "deck_cards": deck_cards, "load_ms": load_ms }),
        )?;
    } else {
        println!(
            "Card DB {}\n  {}\n  {} cards ({deck_cards} deck-eligible)\n  loaded in {load_ms:.0} ms",
            db.id(),
            db.version,
            db.len()
        );
    }
    Ok(ExitCode::SUCCESS)
}

#[derive(Debug, Default, Serialize)]
struct CoverageCounts {
    verified: u64,
    parsed: u64,
    unsupported: u64,
}

fn data_coverage(format: &str, deck: Option<&Path>, args: &DbArgs, json: bool) -> Result<ExitCode> {
    let (db, _) = args.load()?;
    if let Some(path) = deck {
        let report = check_file(&db, path, format.to_owned(), Vec::new())?;
        if json {
            print_json(
                &serde_json::json!({ "deck": report.deck.as_ref().map(|d| &d.id), "card_db": report.card_db, "coverage": report.coverage, "cards": report.cards }),
            )?;
        } else {
            print_coverage(&report);
            for c in &report.cards {
                println!(
                    "  {:<12} {}{}",
                    format!("{:?}", c.coverage).to_lowercase(),
                    c.name,
                    if c.qty > 1 {
                        format!(" ×{}", c.qty)
                    } else {
                        String::new()
                    }
                );
            }
        }
        return Ok(ExitCode::SUCCESS);
    }
    let mut counts = CoverageCounts::default();
    let mut total = 0u64;
    for c in db
        .cards()
        .iter()
        .filter(|c| c.layout.is_deck_card() && c.legality(format) != mtg_data::Legality::NotLegal)
    {
        total += 1;
        match db.coverage(c).level() {
            CoverageLevel::Verified => counts.verified += 1,
            CoverageLevel::Parsed => counts.parsed += 1,
            CoverageLevel::Unsupported => counts.unsupported += 1,
        }
    }
    if json {
        print_json(
            &serde_json::json!({ "format": format, "card_db": db.id(), "cards": total, "coverage": counts }),
        )?;
    } else {
        println!("Coverage of {total} {format}-legal cards in {}:", db.id());
        println!(
            "  verified {} · parsed {} · unsupported {}",
            counts.verified, counts.parsed, counts.unsupported
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn check_file(
    db: &CardDb,
    path: &Path,
    format: String,
    commanders: Vec<String>,
) -> Result<DeckReport> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading decklist {}", path.display()))?;
    let parsed = parse_deck(&text, path.file_name().and_then(|n| n.to_str()));
    let mut parsed = parsed;
    if parsed.name.is_none() {
        parsed.name = path.file_stem().map(|s| s.to_string_lossy().into_owned());
    }
    Ok(validate(
        db,
        &parsed,
        &ValidateOptions { format, commanders },
    ))
}

fn deck_check(
    file: &Path,
    format: String,
    commanders: Vec<String>,
    list_cards: bool,
    args: &DbArgs,
    json: bool,
) -> Result<ExitCode> {
    let (db, _) = args.load()?;
    let report = check_file(&db, file, format, commanders)?;
    if json {
        print_json(&report)?;
    } else {
        let name = report
            .deck
            .as_ref()
            .and_then(|d| d.name.clone())
            .unwrap_or_else(|| file.display().to_string());
        let id = report.deck.as_ref().map(|d| d.id.as_str()).unwrap_or("-");
        println!(
            "{name} (deck {id}): {}",
            if report.legal { "LEGAL" } else { "NOT LEGAL" }
        );
        println!("  format {} · card DB {}", report.format, report.card_db);
        println!(
            "  commander(s): {} · color identity: {} · size: {}",
            if report.commanders.is_empty() {
                "none".to_owned()
            } else {
                report.commanders.join(" + ")
            },
            report.color_identity.as_deref().unwrap_or("-"),
            report.size
        );
        if !report.violations.is_empty() {
            println!("Violations:");
            for v in &report.violations {
                let rule = v.rule.map(|r| format!("[CR {r}] ")).unwrap_or_default();
                println!("  ✗ {rule}{}", v.message);
                for c in v.cards.iter().take(20) {
                    println!("      - {c}");
                }
            }
        }
        if !report.warnings.is_empty() {
            println!("Warnings:");
            for w in &report.warnings {
                let rule = w.rule.map(|r| format!("[CR {r}] ")).unwrap_or_default();
                println!("  ! {rule}{}", w.message);
            }
        }
        print_coverage(&report);
        if list_cards {
            for c in &report.cards {
                let role = if c.commander { " (commander)" } else { "" };
                println!(
                    "  {:<12} {}{}{role}",
                    format!("{:?}", c.coverage).to_lowercase(),
                    c.name,
                    if c.qty > 1 {
                        format!(" ×{}", c.qty)
                    } else {
                        String::new()
                    }
                );
            }
        }
    }
    Ok(if report.legal {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn print_coverage(r: &DeckReport) {
    let c = &r.coverage;
    println!(
        "Coverage: {} verified · {} parsed · {} unsupported → {}",
        c.verified,
        c.parsed,
        c.unsupported,
        if c.experiment_ready {
            "experiment-ready"
        } else {
            "not experiment-ready (experiments refuse unverified cards)"
        }
    );
    for (reason, n) in &c.unsupported_reasons {
        println!("  {n} card(s): {reason}");
    }
}

fn cr_check(cr: &Path, paths: &[PathBuf], require: bool, json: bool) -> Result<ExitCode> {
    if !cr.exists() {
        let msg = format!(
            "CR text not found at {}; download it with the commands in CLAUDE.md",
            cr.display()
        );
        if require {
            bail!(msg);
        }
        eprintln!("skipping cr-check: {msg}");
        return Ok(ExitCode::SUCCESS);
    }
    let tree = mtg_testkit::RuleTree::load(cr)?;
    let (cites, problems) = mtg_testkit::check_tree(&tree, paths)?;
    if json {
        print_json(
            &serde_json::json!({ "cr": tree.effective, "rules": tree.len(), "citations": cites.len(), "problems": problems }),
        )?;
    } else {
        println!(
            "{} ({} rules): checked {} citations",
            tree.effective,
            tree.len(),
            cites.len()
        );
        for p in &problems {
            println!(
                "  ✗ {}:{} CR {}: {}",
                p.citation.file.display(),
                p.citation.line,
                p.citation.text,
                p.problem
            );
        }
        if problems.is_empty() {
            println!("  all citations valid");
        }
    }
    Ok(if problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}
