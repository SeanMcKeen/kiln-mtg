//! Deck checks against the real card DB (M1 exit criterion).
//!
//! Card data is never committed, so these tests run only when a built DB is
//! available: `MTG_CARD_DB=<path>` or `data/cache/current` (from
//! `mtg data build`). Otherwise they skip with a note.

use std::path::{Path, PathBuf};

use mtg_data::CardDb;
use mtg_sim::deck::{DeckReport, ValidateOptions, parse_deck, validate};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn db() -> Option<CardDb> {
    let explicit = std::env::var_os("MTG_CARD_DB").map(PathBuf::from);
    let Ok(path) = CardDb::locate(explicit.as_deref(), &root().join("data/cache")) else {
        eprintln!("skipping: no card DB (set MTG_CARD_DB or run `mtg data build`)");
        return None;
    };
    Some(CardDb::load(&path).unwrap_or_else(|e| panic!("{e}")))
}

fn check(db: &CardDb, file: &str) -> DeckReport {
    let path = root().join("tests/fixtures/decks").join(file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    validate(
        db,
        &parse_deck(&text, Some(file)),
        &ValidateOptions::default(),
    )
}

#[test]
fn kinnan_is_legal_with_full_coverage_report() {
    let Some(db) = db() else { return };
    let r = check(&db, "kinnan.txt");
    assert!(r.legal, "{:#?}", r.violations);
    assert!(r.warnings.is_empty(), "{:#?}", r.warnings);
    assert_eq!(r.size, 100);
    assert_eq!(r.color_identity.as_deref(), Some("UG"));
    assert_eq!(r.commanders, vec!["Kinnan, Bonder Prodigy".to_string()]);
    assert_eq!(r.cards.iter().map(|c| c.qty).sum::<u32>(), 100);
    // M1: no card has a definition yet, and the report says so for every card.
    assert_eq!(r.coverage.unsupported, 100);
    assert!(!r.coverage.experiment_ready);
    // Name collisions resolve to the real cards, not prepare faces or duplicates.
    let brainstorm = r
        .cards
        .iter()
        .find(|c| c.name == "Brainstorm")
        .unwrap_or_else(|| panic!("Brainstorm"));
    assert_eq!(
        db.get(brainstorm.oracle_id).map(|c| c.layout.clone()),
        Some(mtg_data::Layout::Normal)
    );
}

#[test]
fn banned_cards_come_from_data() {
    let Some(db) = db() else { return };
    let r = check(&db, "kinnan-banned.txt");
    assert!(!r.legal);
    let banned = r
        .violations
        .iter()
        .find(|v| v.code == "banned")
        .unwrap_or_else(|| panic!("{:#?}", r.violations));
    // The negative-test group from the Curated cEDH Core Pool note.
    assert_eq!(
        banned.cards,
        vec![
            "Dockside Extortionist",
            "Hullbreacher",
            "Jeweled Lotus",
            "Mana Crypt"
        ]
    );
    let off = r
        .violations
        .iter()
        .find(|v| v.code == "color_identity")
        .unwrap_or_else(|| panic!("color identity"));
    assert_eq!(
        off.cards,
        vec!["Dockside Extortionist (R)", "Lightning Bolt (R)"]
    );
}

#[test]
fn typos_and_counts() {
    let Some(db) = db() else { return };
    let r = check(&db, "kinnan-errors.txt");
    let codes: Vec<&str> = r.violations.iter().map(|v| v.code).collect();
    assert_eq!(
        codes,
        vec!["unknown_card", "unknown_card", "deck_size", "singleton"]
    );
    assert!(
        r.violations[0]
            .cards
            .first()
            .is_some_and(|s| s == "Rhystic Study")
    );
    assert!(r.warnings.iter().any(|w| w.code == "loose_name_match" && w.cards == vec!["Thassa's Oracle".to_string()]));
}
