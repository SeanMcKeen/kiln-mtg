//! Real-card verification (M3 plan, B-D13).
//!
//! A card is *verified* when it has an IR definition in `cards/defs/`, the
//! engine supports all of it, and CR-cited scenarios in
//! `tests/scenarios-real/` that list it under `verifies:` pass. Real cards'
//! characteristics come from the local card DB, which is never committed,
//! so this runs only where a DB exists (Q25).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mtg_cards::defs::{CardFile, load_dir};
use mtg_data::{CardDb, NameMatch};
use mtg_engine::{CardDef, CardPool};
use serde::Serialize;

use crate::scenario;

/// The repository root.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The local card DB: `MTG_CARD_DB`, else `data/cache/current`. `None` if there's none.
pub fn locate_db() -> Option<CardDb> {
    let explicit = std::env::var_os("MTG_CARD_DB").map(PathBuf::from);
    let path = CardDb::locate(explicit.as_deref(), &repo_root().join("data/cache")).ok()?;
    CardDb::load(&path).ok()
}

/// Lands built from the DB without IR definitions (their only abilities are
/// intrinsic, CR 305.6).
pub const PLAIN_LANDS: [&str; 15] = [
    "Plains",
    "Island",
    "Swamp",
    "Mountain",
    "Forest",
    "Tundra",
    "Underground Sea",
    "Badlands",
    "Taiga",
    "Savannah",
    "Scrubland",
    "Volcanic Island",
    "Bayou",
    "Plateau",
    "Tropical Island",
];

/// Why a defined card isn't in the pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rejected {
    pub card: String,
    pub reason: String,
}

/// The synthetic M3 test set plus real cards: the plain lands and every
/// supported definition. Returns the pool and the definitions it couldn't use.
pub fn real_pool(db: &CardDb, defs: &[CardFile]) -> (CardPool, Vec<Rejected>) {
    add_real_cards(CardPool::m3_test_set(), db, defs)
}

/// Only real cards: the plain lands and every supported definition (for fuzzing decks of verified cards).
pub fn real_only_pool(db: &CardDb, defs: &[CardFile]) -> (CardPool, Vec<Rejected>) {
    add_real_cards(CardPool::new(), db, defs)
}

fn add_real_cards(mut pool: CardPool, db: &CardDb, defs: &[CardFile]) -> (CardPool, Vec<Rejected>) {
    let mut rejected = Vec::new();
    let record = |name: &str| match db.resolve(name, "commander") {
        NameMatch::Exact(i) => Ok(&db.cards()[i]),
        other => Err(format!("not found in the card DB: {other:?}")),
    };
    for name in PLAIN_LANDS {
        match record(name).and_then(|r| CardDef::from_record(r).map_err(|e| e.reason)) {
            Ok(def) => {
                pool.add(def);
            }
            Err(reason) => rejected.push(Rejected {
                card: name.into(),
                reason,
            }),
        }
    }
    for f in defs {
        let built = record(&f.card).and_then(|r| {
            let face = f.faces.first().ok_or("no faces")?;
            let def = CardDef::from_record_ir(r, face).map_err(|e| e.reason)?;
            pool.try_add(def).map_err(|e| e.reason)
        });
        if let Err(reason) = built {
            rejected.push(Rejected {
                card: f.card.clone(),
                reason,
            });
        }
    }
    (pool, rejected)
}

/// The verification report.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub definitions: usize,
    /// Defined cards the engine can't run yet, with reasons.
    pub unsupported: Vec<Rejected>,
    /// Verified cards and the scenarios that verify them.
    pub verified: BTreeMap<String, Vec<String>>,
    /// Supported definitions no passing scenario verifies yet.
    pub unverified: Vec<String>,
    /// Scenario files that failed, with the error.
    pub failures: Vec<(String, String)>,
}

/// Loads `cards/defs/`, builds the real pool and runs every real-card scenario.
pub fn verify(db: &CardDb) -> Result<Report, String> {
    let root = repo_root();
    let defs = load_dir(&root.join("cards/defs")).map_err(|e| e.to_string())?;
    let (pool, unsupported) = real_pool(db, &defs);
    let pool = Arc::new(pool);
    let mut report = Report {
        definitions: defs.len(),
        unsupported,
        ..Report::default()
    };
    let dir = root.join("tests/scenarios-real");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "yaml"))
        .collect();
    files.sort();
    for f in &files {
        let label = f
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let result = scenario::load(f).and_then(|s| {
            if s.cr.is_empty() {
                return Err(scenario::ScenarioError::Setup("cites no CR rule".into()));
            }
            scenario::run(&s, &pool).map(|_| s)
        });
        match result {
            Ok(s) => {
                for card in s.verifies {
                    report.verified.entry(card).or_default().push(label.clone());
                }
            }
            Err(e) => report.failures.push((label, e.to_string())),
        }
    }
    let bad: Vec<&str> = report.unsupported.iter().map(|r| r.card.as_str()).collect();
    // Only supported cards count; a scenario can't verify a card the pool lacks.
    report
        .verified
        .retain(|card, _| pool.by_name(card).is_some() && !bad.contains(&card.as_str()));
    report.unverified = defs
        .iter()
        .map(|d| d.card.clone())
        .filter(|c| !bad.contains(&c.as_str()) && !report.verified.contains_key(c))
        .collect();
    Ok(report)
}
