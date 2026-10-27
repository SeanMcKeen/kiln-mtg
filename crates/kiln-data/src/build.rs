//! The build pipeline: bulk file → normalized records → card DB + report
//! (`docs/06 Card Data/Ingestion Pipeline.md`).

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::db::{CardDb, CardDbVersion, DbError, PIPELINE_VERSION};
use crate::normalize::normalize;
use crate::record::{CardRecord, Layout};
use crate::scryfall::{RawCard, RawRuling, ReadError, read_bulk};

/// Inputs to a build.
#[derive(Debug, Clone)]
pub struct BuildInput<'a> {
    pub oracle_cards: &'a Path,
    pub rulings: Option<&'a Path>,
    /// e.g. `scryfall/oracle_cards`.
    pub source: String,
    /// The source's date, or `unknown`.
    pub source_date: String,
}

/// What the build saw. Rendered as JSON and as a markdown summary.
#[derive(Debug, Clone, Serialize)]
pub struct BuildReport {
    pub db_id: String,
    pub version: CardDbVersion,
    pub records_read: usize,
    pub records_skipped_no_oracle_id: usize,
    pub cards: usize,
    pub deck_cards: usize,
    pub layouts: BTreeMap<String, usize>,
    pub unknown_layouts: BTreeMap<String, usize>,
    pub cards_with_issues: usize,
    /// Up to 50 example issues, `name: issue`.
    pub issue_examples: Vec<String>,
    /// Deck cards whose recomputed color identity differs from the source's.
    pub color_identity_mismatches: Vec<ColorIdentityMismatch>,
    /// Normalized names that map to more than one deck card.
    pub duplicate_names: Vec<DuplicateName>,
    /// Face names shared by several cards or colliding with another card's full name.
    /// Full names always resolve first, so these only matter when typed alone.
    pub shadowed_face_names: Vec<DuplicateName>,
    pub rulings_attached: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ColorIdentityMismatch {
    pub name: String,
    pub source: String,
    pub computed: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateName {
    pub name: String,
    pub candidates: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("can't hash {path}: {source}")]
    Hash {
        path: String,
        source: std::io::Error,
    },
}

/// Runs the pipeline. Pure apart from reading the inputs.
pub fn build(input: &BuildInput<'_>) -> Result<(CardDb, BuildReport), BuildError> {
    let raw: Vec<RawCard> = read_bulk(input.oracle_cards)?;
    let input_sha256 = sha256_file(input.oracle_cards)?;
    let records_read = raw.len();
    let mut cards: Vec<CardRecord> = raw.iter().filter_map(normalize).collect();
    let records_skipped_no_oracle_id = records_read - cards.len();

    let rulings_attached = match input.rulings {
        Some(path) => {
            let rulings: Vec<RawRuling> = read_bulk(path)?;
            let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
            for r in &rulings {
                *counts.entry(r.oracle_id.as_str()).or_default() += 1;
            }
            for c in &mut cards {
                c.rulings = Some(
                    counts
                        .get(c.oracle_id.to_string().as_str())
                        .copied()
                        .unwrap_or(0),
                );
            }
            Some(rulings.len())
        }
        None => None,
    };

    let version = CardDbVersion {
        source: input.source.clone(),
        source_date: input.source_date.clone(),
        pipeline_version: PIPELINE_VERSION,
        overrides_hash: "none".into(),
        input_sha256,
    };
    let db = CardDb::new(version, cards)?;
    let report = report(
        &db,
        records_read,
        records_skipped_no_oracle_id,
        rulings_attached,
    );
    Ok((db, report))
}

fn report(
    db: &CardDb,
    records_read: usize,
    skipped: usize,
    rulings_attached: Option<usize>,
) -> BuildReport {
    let mut layouts = BTreeMap::new();
    let mut unknown_layouts = BTreeMap::new();
    let mut issue_examples = Vec::new();
    let mut cards_with_issues = 0;
    let mut mismatches = Vec::new();
    for c in db.cards() {
        *layouts.entry(c.layout.to_string()).or_insert(0) += 1;
        if let Layout::Unknown(l) = &c.layout {
            *unknown_layouts.entry(l.clone()).or_insert(0) += 1;
        }
        if !c.issues.is_empty() {
            cards_with_issues += 1;
            for i in &c.issues {
                if issue_examples.len() < 50 {
                    issue_examples.push(format!("{}: {i}", c.name));
                }
            }
        }
        if c.layout.is_deck_card() && c.color_identity != c.computed_color_identity {
            mismatches.push(ColorIdentityMismatch {
                name: c.name.clone(),
                source: c.color_identity.to_string(),
                computed: c.computed_color_identity.to_string(),
            });
        }
    }
    let duplicate_names = db
        .names()
        .duplicate_names()
        .map(|(name, hits)| DuplicateName {
            name: name.to_owned(),
            candidates: hits
                .iter()
                .map(|&i| {
                    let c = &db.cards()[i];
                    format!(
                        "{} [{}; commander={:?}; games={}]",
                        c.oracle_id,
                        c.set_type,
                        c.legality("commander"),
                        c.games.join(",")
                    )
                })
                .collect(),
        })
        .collect();
    let shadowed_face_names = db
        .names()
        .shadowed_face_names()
        .map(|(name, hits)| DuplicateName {
            name: name.to_owned(),
            candidates: hits.iter().map(|&i| db.cards()[i].name.clone()).collect(),
        })
        .collect();
    BuildReport {
        db_id: db.id(),
        version: db.version.clone(),
        records_read,
        records_skipped_no_oracle_id: skipped,
        cards: db.len(),
        deck_cards: db
            .cards()
            .iter()
            .filter(|c| c.layout.is_deck_card())
            .count(),
        layouts,
        unknown_layouts,
        cards_with_issues,
        issue_examples,
        color_identity_mismatches: mismatches,
        duplicate_names,
        shadowed_face_names,
        rulings_attached,
    }
}

impl BuildReport {
    /// A short markdown summary for humans and agents.
    pub fn to_markdown(&self) -> String {
        let mut s = format!("# Card DB build `{}`\n\n", self.db_id);
        s += &format!(
            "- Source: {} ({}), pipeline v{}\n",
            self.version.source, self.version.source_date, self.version.pipeline_version
        );
        s += &format!("- Input SHA-256: `{}`\n", self.version.input_sha256);
        s += &format!(
            "- Records read: {} (skipped without Oracle ID: {})\n",
            self.records_read, self.records_skipped_no_oracle_id
        );
        s += &format!(
            "- Cards: {} ({} deck-eligible)\n",
            self.cards, self.deck_cards
        );
        s += &format!("- Cards with data issues: {}\n", self.cards_with_issues);
        s += &format!(
            "- Color identity mismatches (computed vs source): {}\n",
            self.color_identity_mismatches.len()
        );
        s += &format!(
            "- Duplicate deck-card names: {}\n",
            self.duplicate_names.len()
        );
        s += &format!(
            "- Shadowed face names: {}\n",
            self.shadowed_face_names.len()
        );
        if let Some(n) = self.rulings_attached {
            s += &format!("- Rulings attached: {n}\n");
        }
        s += "\n## Layouts\n\n| Layout | Cards |\n|---|---|\n";
        for (l, n) in &self.layouts {
            s += &format!("| {l} | {n} |\n");
        }
        if !self.unknown_layouts.is_empty() {
            s += "\n## Unknown layouts (unsupported)\n\n";
            for (l, n) in &self.unknown_layouts {
                s += &format!("- `{l}`: {n}\n");
            }
        }
        if !self.color_identity_mismatches.is_empty() {
            s += "\n## Color identity mismatches (first 25)\n\n| Card | Source | Computed |\n|---|---|---|\n";
            for m in self.color_identity_mismatches.iter().take(25) {
                s += &format!("| {} | {} | {} |\n", m.name, m.source, m.computed);
            }
        }
        s
    }
}

fn sha256_file(path: &Path) -> Result<String, BuildError> {
    let bytes = std::fs::read(path).map_err(|source| BuildError::Hash {
        path: path.display().to_string(),
        source,
    })?;
    Ok(crate::db::hex(&Sha256::digest(&bytes)))
}
