//! Resolving an experiment's decks: sources, legality and the coverage gate
//! (FR-S8, M7 plan S3), and A/B variants (S13).

use std::path::Path;

use kiln_cards::CoverageLevel;
use kiln_data::CardDb;
use kiln_protocol::spec::{CoverageGate, DeckSource, Mode, Spec};
use kiln_types::OracleId;

use super::ExperimentError;
use crate::deck::{
    DeckEntry, DeckReport, ParsedDeck, Section, ValidateOptions, parse_deck, validate_with,
};
use crate::library::Library;

/// A deck ready to play: a spec deck or an A/B variant of one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDeck {
    /// The spec's deck id, or the variant's name.
    pub id: String,
    /// For a variant, the deck it changes.
    pub base: Option<String>,
    pub name: Option<String>,
    pub commanders: Vec<OracleId>,
    /// Non-commander cards and quantities, sorted by Oracle ID.
    pub cards: Vec<(OracleId, u32)>,
    pub report: DeckReport,
}

/// Reads, validates and gates every deck, plus `mode: ab` variants. Returns the
/// decks (spec order, then variants by name) and warnings.
pub fn resolve_decks(
    spec: &Spec,
    spec_dir: &Path,
    db: &CardDb,
    lib: &Library,
) -> Result<(Vec<ResolvedDeck>, Vec<String>), ExperimentError> {
    let mut decks = Vec::new();
    let mut warnings = Vec::new();
    let mut parsed_by_id = Vec::new();
    for d in &spec.decks {
        let (text, filename) = match &d.source {
            DeckSource::File(f) => {
                let path = spec_dir.join(f);
                let text = std::fs::read_to_string(&path).map_err(|e| ExperimentError::Deck {
                    id: d.id.clone(),
                    message: format!("can't read {}: {e}", path.display()),
                })?;
                (text, Some(f.clone()))
            }
            DeckSource::Text(t) | DeckSource::MoxfieldExport(t) => (t.clone(), None),
        };
        let parsed = parse_deck(&text, filename.as_deref());
        let deck = check(spec, db, lib, &d.id, None, &parsed, &mut warnings)?;
        parsed_by_id.push((d.id.clone(), parsed));
        decks.push(deck);
    }
    if spec.mode == Mode::Ab {
        for (name, v) in &spec.variants {
            let Some((_, base)) = parsed_by_id.iter().find(|(id, _)| *id == v.base) else {
                continue; // validate() already rejected an unknown base
            };
            let parsed = swapped(base, &v.swap.out, &v.swap.add).map_err(|message| {
                ExperimentError::Deck {
                    id: name.clone(),
                    message,
                }
            })?;
            let deck = check(
                spec,
                db,
                lib,
                name,
                Some(v.base.clone()),
                &parsed,
                &mut warnings,
            )?;
            decks.push(deck);
        }
    }
    Ok((decks, warnings))
}

/// Validates one deck and applies the coverage gate.
fn check(
    spec: &Spec,
    db: &CardDb,
    lib: &Library,
    id: &str,
    base: Option<String>,
    parsed: &ParsedDeck,
    warnings: &mut Vec<String>,
) -> Result<ResolvedDeck, ExperimentError> {
    let report = validate_with(
        db,
        parsed,
        &ValidateOptions {
            format: spec.format.clone(),
            commanders: Vec::new(),
        },
        &|c| lib.coverage(c.oracle_id),
    );
    let Some(record) = report.deck.clone().filter(|_| report.legal) else {
        let problems: Vec<String> = report
            .violations
            .iter()
            .map(|v| v.message.clone())
            .collect();
        return Err(ExperimentError::Deck {
            id: id.to_owned(),
            message: format!("not a legal {} deck: {}", spec.format, problems.join("; ")),
        });
    };
    let below = |level: CoverageLevel| -> Vec<String> {
        report
            .cards
            .iter()
            .filter(|c| c.coverage <= level)
            .map(|c| c.name.clone())
            .collect()
    };
    // Cards the engine can't run at all are never allowed.
    let unsupported = below(CoverageLevel::Unsupported);
    if !unsupported.is_empty() {
        return Err(ExperimentError::Coverage {
            id: id.to_owned(),
            cards: unsupported,
            gate: "unsupported cards can't be played".into(),
        });
    }
    let unverified = below(CoverageLevel::Parsed);
    if !unverified.is_empty() {
        match spec.limits.coverage {
            CoverageGate::RequireVerified => {
                return Err(ExperimentError::Coverage {
                    id: id.to_owned(),
                    cards: unverified,
                    gate: "limits.coverage is require_verified (use warn or allow to run anyway)"
                        .into(),
                });
            }
            CoverageGate::Warn => warnings.push(format!(
                "{} card(s) in '{id}' are parsed but not verified: {}",
                unverified.len(),
                unverified.join(", ")
            )),
            CoverageGate::Allow => {}
        }
    }
    Ok(ResolvedDeck {
        id: id.to_owned(),
        base,
        name: record.name.clone(),
        commanders: record.commanders.clone(),
        cards: record.cards.clone(),
        report,
    })
}

/// A copy of `deck` with one of each `out` card removed and each `add` card added.
fn swapped(deck: &ParsedDeck, out: &[String], add: &[String]) -> Result<ParsedDeck, String> {
    let mut deck = deck.clone();
    for name in out {
        let key = name.to_lowercase();
        let Some(i) = deck
            .entries
            .iter()
            .position(|e| e.section == Section::Main && e.name.to_lowercase() == key)
        else {
            return Err(format!("swap.out: {name} isn't in the base deck"));
        };
        if deck.entries[i].qty > 1 {
            deck.entries[i].qty -= 1;
        } else {
            deck.entries.remove(i);
        }
    }
    for name in add {
        deck.entries.push(DeckEntry {
            qty: 1,
            name: name.clone(),
            section: Section::Main,
            line: 0,
            commander_marker: false,
        });
    }
    Ok(deck)
}
