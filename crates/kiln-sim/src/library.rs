//! The card library (ADR-0007, M3 plan C1): one definition per card from the
//! hybrid front-end.
//!
//! Precedence, per card:
//! 1. a YAML override in `cards/defs/` (provenance `override`);
//! 2. otherwise the Oracle-text parser, accepted only if every line parses,
//!    every parsed line passes the renderer lint, and the engine supports the
//!    result (provenance `parser`);
//! 3. otherwise `unsupported`, with every reason.
//!
//! A card is `verified` when its definition is supported and a real-card
//! scenario verifies it (supplied by the caller); otherwise `parsed`.

use std::collections::{BTreeMap, BTreeSet};

use kiln_cards::defs::CardFile;
use kiln_cards::ir::FaceIr;
use kiln_cards::parse::parse_face_checked;
use kiln_cards::{CardDefinition, Coverage, CoverageLevel, Provenance, UnsupportedSpan};
use kiln_data::{CardDb, CardRecord};
use kiln_engine::{CardDef, CardPool, DefId};
use kiln_types::{CardType, OracleId};
use serde::Serialize;

/// The parser's version, recorded in provenance.
pub const GRAMMAR_VERSION: &str = "m3-1";

/// One card's definition and engine form.
#[derive(Debug, Clone)]
pub struct LibraryCard {
    pub name: String,
    pub definition: CardDefinition,
    /// The engine definition, when the card is supported.
    pub def: Option<CardDef>,
}

/// Coverage counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub verified: u64,
    pub parsed: u64,
    pub unsupported: u64,
    /// Supported cards by provenance.
    pub from_overrides: u64,
    pub from_parser: u64,
}

/// Every deck card's definition.
#[derive(Debug, Clone, Default)]
pub struct Library {
    cards: BTreeMap<OracleId, LibraryCard>,
}

fn unsupported(id: OracleId, reasons: Vec<UnsupportedSpan>) -> CardDefinition {
    CardDefinition {
        oracle_id: id,
        provenance: Provenance::None,
        coverage: Coverage::Unsupported { reasons },
    }
}

impl Library {
    /// Builds definitions for every deck card in `db`. `overrides` are the
    /// YAML definitions; `verified` names the cards real-card scenarios verify.
    pub fn build(db: &CardDb, overrides: &[CardFile], verified: &BTreeSet<String>) -> Library {
        let by_name: BTreeMap<&str, &CardFile> =
            overrides.iter().map(|f| (f.card.as_str(), f)).collect();
        let mut cards = BTreeMap::new();
        for rec in db.cards().iter().filter(|c| c.layout.is_deck_card()) {
            let card = Self::define(rec, by_name.get(rec.name.as_str()).copied(), verified);
            cards.insert(rec.oracle_id, card);
        }
        Library { cards }
    }

    fn define(
        rec: &CardRecord,
        over: Option<&CardFile>,
        verified: &BTreeSet<String>,
    ) -> LibraryCard {
        let id = rec.oracle_id;
        let done = |provenance: Provenance, def: CardDef| {
            let coverage = if verified.contains(&rec.name) {
                Coverage::Verified {
                    tests: Vec::new(),
                    since: "M3".into(),
                }
            } else {
                Coverage::Parsed
            };
            LibraryCard {
                name: rec.name.clone(),
                definition: CardDefinition {
                    oracle_id: id,
                    provenance,
                    coverage,
                },
                def: Some(def),
            }
        };
        let fail = |reasons: Vec<UnsupportedSpan>| LibraryCard {
            name: rec.name.clone(),
            definition: unsupported(id, reasons),
            def: None,
        };
        if let Some(f) = over {
            let face = f.faces.first().cloned().unwrap_or_default();
            return match CardDef::from_record_ir(rec, &face) {
                Ok(def) => done(
                    Provenance::Override {
                        file: format!("cards/defs/{}", f.card),
                        author: "repository".into(),
                    },
                    def,
                ),
                Err(e) => fail(vec![UnsupportedSpan::whole_card(e.reason)]),
            };
        }
        if rec.faces.len() != 1 {
            return fail(vec![UnsupportedSpan::whole_card("multi-face layouts")]);
        }
        let front = rec.front();
        let is_spell = front.type_line.has_type(CardType::Instant)
            || front.type_line.has_type(CardType::Sorcery);
        let parsed = parse_face_checked(&front.reminder_stripped, &rec.name, is_spell);
        if !parsed.complete() {
            let reasons = parsed
                .lines
                .iter()
                .filter(|(_, ok)| !ok)
                .map(|(line, _)| UnsupportedSpan {
                    face: None,
                    text: Some(line.clone()),
                    reason: if parsed.lint_failures.contains(line) {
                        "renderer lint mismatch".into()
                    } else {
                        "no parser template".into()
                    },
                })
                .collect();
            return fail(reasons);
        }
        let face = FaceIr {
            abilities: parsed.abilities,
        };
        match CardDef::from_record_ir(rec, &face) {
            Ok(def) => done(
                Provenance::Parser {
                    grammar_version: GRAMMAR_VERSION.into(),
                },
                def,
            ),
            Err(e) => fail(vec![UnsupportedSpan::whole_card(e.reason)]),
        }
    }

    pub fn get(&self, id: OracleId) -> Option<&LibraryCard> {
        self.cards.get(&id)
    }

    /// The card's coverage; cards the library doesn't know are unsupported.
    pub fn coverage(&self, id: OracleId) -> Coverage {
        self.get(id).map_or_else(
            || Coverage::Unsupported {
                reasons: vec![UnsupportedSpan::whole_card("not a deck card")],
            },
            |c| c.definition.coverage.clone(),
        )
    }

    /// Counts over the cards `filter` accepts.
    pub fn counts(&self, db: &CardDb, filter: impl Fn(&CardRecord) -> bool) -> Counts {
        let mut n = Counts::default();
        for rec in db.cards().iter().filter(|c| filter(c)) {
            let Some(c) = self.get(rec.oracle_id) else {
                continue;
            };
            match c.definition.coverage.level() {
                CoverageLevel::Verified => n.verified += 1,
                CoverageLevel::Parsed => n.parsed += 1,
                CoverageLevel::Unsupported => n.unsupported += 1,
            }
            match c.definition.provenance {
                Provenance::Override { .. } => n.from_overrides += 1,
                Provenance::Parser { .. } => n.from_parser += 1,
                _ => {}
            }
        }
        n
    }

    /// The most common unsupported reasons over the cards `filter` accepts.
    pub fn top_reasons(
        &self,
        db: &CardDb,
        filter: impl Fn(&CardRecord) -> bool,
        limit: usize,
    ) -> Vec<(String, u64)> {
        let mut by: BTreeMap<String, u64> = BTreeMap::new();
        for rec in db.cards().iter().filter(|c| filter(c)) {
            if let Some(LibraryCard {
                definition:
                    CardDefinition {
                        coverage: Coverage::Unsupported { reasons },
                        ..
                    },
                ..
            }) = self.get(rec.oracle_id)
            {
                for r in reasons {
                    *by.entry(r.reason.clone()).or_default() += 1;
                }
            }
        }
        let mut v: Vec<(String, u64)> = by.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v.truncate(limit);
        v
    }

    /// A pool of every supported card (tokens added as needed), and the
    /// pool ids by oracle id.
    pub fn pool(&self) -> (CardPool, BTreeMap<OracleId, DefId>) {
        let mut pool = CardPool::new();
        let mut ids = BTreeMap::new();
        for (oid, c) in &self.cards {
            if let Some(def) = &c.def
                && let Ok(id) = pool.try_add(def.clone())
            {
                ids.insert(*oid, id);
            }
        }
        (pool, ids)
    }
}
