//! Pipeline tests on the synthetic fixture `tests/fixtures/cards-mini.jsonl`
//! (made-up cards in Scryfall's shape; no real card data is committed).

use std::path::{Path, PathBuf};

use mtg_cards::CoverageLevel;
use mtg_data::names::NameMatch;
use mtg_data::{BuildInput, CardDb, Layout, Legality, build};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/cards-mini.jsonl")
}

fn db() -> (CardDb, mtg_data::BuildReport) {
    let path = fixture();
    let input = BuildInput {
        oracle_cards: &path,
        rulings: None,
        source: "fixture/cards-mini".into(),
        source_date: "2026-10-20".into(),
    };
    build(&input).unwrap_or_else(|e| panic!("{e}"))
}

fn name_of(db: &CardDb, m: &NameMatch) -> String {
    match m {
        NameMatch::Exact(i) | NameMatch::Loose(i) => db.cards()[*i].name.clone(),
        other => format!("{other:?}"),
    }
}

#[test]
fn counts_and_layouts() {
    let (db, report) = db();
    assert_eq!(report.records_read, 38);
    assert_eq!(db.len(), 38);
    // token + art series aren't deck cards
    assert_eq!(report.deck_cards, 36);
    assert_eq!(report.unknown_layouts.get("hologram"), Some(&1));
    let odd = db
        .cards()
        .iter()
        .find(|c| c.name == "Shimmering Oddity")
        .unwrap_or_else(|| panic!("card"));
    assert_eq!(odd.layout, Layout::Unknown("hologram".into()));
    let reasons = match db.coverage(odd) {
        mtg_cards::Coverage::Unsupported { reasons } => reasons,
        other => panic!("{other:?}"),
    };
    assert!(reasons.iter().any(|r| r.reason.contains("hologram")));
}

#[test]
fn every_card_is_unsupported_in_m1() {
    let (db, _) = db();
    assert!(
        db.cards()
            .iter()
            .all(|c| db.coverage(c).level() == CoverageLevel::Unsupported)
    );
}

#[test]
fn name_resolution() {
    let (db, _) = db();
    // Duplicate names: prefer the Commander-legal paper object over the joke one.
    let twin = db.resolve("Twin Name", "commander");
    let NameMatch::Exact(i) = twin else {
        panic!("{twin:?}")
    };
    assert_eq!(db.cards()[i].legality("commander"), Legality::Legal);
    assert_eq!(db.cards()[i].set_type, "expansion");
    // The art-series object with the same face names is never a deck card.
    assert_eq!(
        name_of(&db, &db.resolve("thicket guardian", "commander")),
        "Thicket Guardian"
    );
    // Face names and A/B spellings of split cards.
    assert_eq!(
        name_of(&db, &db.resolve("Ember", "commander")),
        "Spark // Ember"
    );
    assert_eq!(
        name_of(&db, &db.resolve("Spark/Ember", "commander")),
        "Spark // Ember"
    );
    // Adventure/prepare cards are named by their full or front-face name, not the spell face.
    assert_eq!(
        name_of(&db, &db.resolve("Garden Knight", "commander")),
        "Garden Knight // Weed Out"
    );
    assert!(matches!(
        db.resolve("Weed Out", "commander"),
        NameMatch::NotFound { .. }
    ));
    // A prepare card's spell face reusing another card's name never shadows that card
    // (real case: "Harmonized Trio // Brainstorm" vs Brainstorm).
    assert_eq!(
        name_of(&db, &db.resolve("Glimmer Bolt", "commander")),
        "Glimmer Bolt"
    );
    assert_eq!(
        name_of(&db, &db.resolve("Ember Adept", "commander")),
        "Ember Adept // Glimmer Bolt"
    );
    // Ember Adept's front face is indexed; its spell face isn't, so nothing is shadowed by it.
    assert!(
        db.names()
            .shadowed_face_names()
            .all(|(n, _)| n != "glimmer bolt")
    );
    // Punctuation-insensitive matches are flagged as loose.
    assert!(matches!(
        db.resolve("Lorekeepers Tome", "commander"),
        NameMatch::Loose(_)
    ));
    // Misspellings get suggestions, never a silent pick.
    match db.resolve("Thicket Gaurdian", "commander") {
        NameMatch::NotFound { suggestions } => assert_eq!(
            suggestions.first().map(String::as_str),
            Some("Thicket Guardian")
        ),
        other => panic!("{other:?}"),
    }
    // Tokens can't be resolved for decks.
    assert!(matches!(
        db.resolve("Spirit", "commander"),
        NameMatch::NotFound { .. }
    ));
}

#[test]
fn color_identity_recomputation() {
    let (db, report) = db();
    let card = |n: &str| {
        db.cards()
            .iter()
            .find(|c| c.name == n)
            .unwrap_or_else(|| panic!("{n}"))
    };
    // CR 903.4e: adventure characteristics count.
    assert_eq!(
        card("Garden Knight // Weed Out")
            .computed_color_identity
            .to_string(),
        "WG"
    );
    // CR 903.4d: back face, including its color indicator.
    assert_eq!(
        card("Moonlit Pup // Moonlit Howler")
            .computed_color_identity
            .to_string(),
        "BG"
    );
    // Hybrid and Phyrexian symbols.
    assert_eq!(card("Brook Sage").computed_color_identity.to_string(), "UG");
    assert_eq!(
        card("Thorned Remnant").computed_color_identity.to_string(),
        "G"
    );
    // CR 903.4c: reminder text is ignored, so lands with only a reminder-text
    // mana ability are colorless by 903.4; 903.5d handles their basic land types.
    assert!(card("Murky Bog").computed_color_identity.is_colorless());
    assert!(card("Hollow Wyrm").computed_color_identity.is_colorless());
    // The source gives basics an identity; that difference is reported, not hidden.
    assert!(
        report
            .color_identity_mismatches
            .iter()
            .any(|m| m.name == "Wavefield")
    );
}

#[test]
fn builds_are_deterministic_and_round_trip() {
    let (a, _) = db();
    let (b, _) = db();
    let bytes = a.to_bytes().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(bytes, b.to_bytes().unwrap_or_else(|e| panic!("{e}")));
    assert_eq!(a.content_sha256, b.content_sha256);

    // Input order doesn't matter.
    let mut reversed = a.cards().to_vec();
    reversed.reverse();
    let c = CardDb::new(a.version.clone(), reversed).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(c.content_sha256, a.content_sha256);

    let back = CardDb::from_bytes(&bytes, "mem").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(back.content_sha256, a.content_sha256);
    assert_eq!(back.cards(), a.cards());
    assert!(a.id().starts_with("fixture-2026-10-20#"));
}

#[test]
fn rejects_garbage_files() {
    assert!(matches!(
        CardDb::from_bytes(b"not a db at all", "x"),
        Err(mtg_data::DbError::BadMagic { .. })
    ));
    let mut bad_version = b"MTGCARDB".to_vec();
    bad_version.extend_from_slice(&99u32.to_le_bytes());
    assert!(matches!(
        CardDb::from_bytes(&bad_version, "x"),
        Err(mtg_data::DbError::FormatVersion { found: 99, .. })
    ));
}

#[test]
fn write_and_locate() {
    let (db, _) = db();
    let dir = std::env::temp_dir().join(format!("mtg-data-test-{}", std::process::id()));
    let file = db.write_to_dir(&dir).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        file.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("carddb-2026-10-20-"))
    );
    let located = CardDb::locate(None, &dir).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(located, file);
    let loaded = CardDb::load(&located).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(loaded.content_sha256, db.content_sha256);
    let _ = std::fs::remove_dir_all(&dir);
}
