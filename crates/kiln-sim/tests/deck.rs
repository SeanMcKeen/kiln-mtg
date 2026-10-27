//! Commander deck validation against the synthetic fixture card DB.

use std::path::Path;
use std::sync::LazyLock;

use kiln_data::{BuildInput, CardDb, build};
use kiln_sim::deck::{DeckReport, ValidateOptions, parse_text, validate};

static DB: LazyLock<CardDb> = LazyLock::new(|| {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/cards-mini.jsonl");
    let input = BuildInput {
        oracle_cards: &path,
        rulings: None,
        source: "fixture/cards-mini".into(),
        source_date: "2026-10-20".into(),
    };
    build(&input).unwrap_or_else(|e| panic!("{e}")).0
});

/// A legal 100-card GU deck led by Vexa, with `extra` lines replacing basics one-for-one.
fn deck_text(commanders: &[&str], extra: &[&str]) -> String {
    let mut s = String::from("Commander\n");
    for c in commanders {
        s += &format!("1 {c}\n");
    }
    s += "Deck\n1 Brook Sage\n1 Thicket Guardian\n1 Tidecaller\n7 Seven Brothers\n";
    let mut nonbasic = 10 + commanders.len() as u32;
    for e in extra {
        s += e;
        s.push('\n');
        nonbasic += e
            .split_whitespace()
            .next()
            .and_then(|q| q.parse::<u32>().ok())
            .unwrap_or(1);
    }
    let basics = 100 - nonbasic;
    s += &format!(
        "{} Wavefield\n{} Grovefield\n",
        basics / 2,
        basics - basics / 2
    );
    s
}

fn check(text: &str) -> DeckReport {
    validate(&DB, &parse_text(text), &ValidateOptions::default())
}

fn codes(r: &DeckReport) -> Vec<&'static str> {
    r.violations.iter().map(|v| v.code).collect()
}

#[test]
fn legal_deck() {
    let r = check(&deck_text(&["Vexa, Tide Scholar"], &[]));
    assert!(r.legal, "{:#?}", r.violations);
    assert_eq!(r.size, 100);
    assert_eq!(r.color_identity.as_deref(), Some("UG"));
    assert_eq!(r.commanders, vec!["Vexa, Tide Scholar".to_string()]);
    assert_eq!(r.coverage.unsupported, 100);
    assert!(!r.coverage.experiment_ready);
    assert!(
        r.deck
            .as_ref()
            .is_some_and(|d| d.commanders.len() == 1 && d.hash.len() == 64)
    );
}

#[test]
fn deck_hash_ignores_line_order() {
    let a = check("Commander\n1 Vexa, Tide Scholar\nDeck\n1 Brook Sage\n2 Wavefield\n");
    let b = check("2 Wavefield\n1 Brook Sage\n1 Vexa, Tide Scholar *CMDR*\n");
    assert_eq!(a.deck.map(|d| d.hash), b.deck.map(|d| d.hash));
}

#[test]
fn size_banned_identity_singleton() {
    let r = check(&deck_text(
        &["Vexa, Tide Scholar"],
        &[
            "1 Pact Breaker",
            "1 Glimmer Bolt",
            "1 Thicket Guardian",
            "1 Seven Brothers",
        ],
    ));
    let c = codes(&r);
    assert!(!r.legal);
    for code in ["banned", "color_identity", "singleton"] {
        assert!(c.contains(&code), "missing {code} in {c:?}");
    }
    let singleton = r
        .violations
        .iter()
        .find(|v| v.code == "singleton")
        .unwrap_or_else(|| panic!("singleton"));
    assert!(
        singleton
            .cards
            .iter()
            .any(|s| s.starts_with("Thicket Guardian ×2"))
    );
    assert!(
        singleton
            .cards
            .iter()
            .any(|s| s.starts_with("Seven Brothers ×8"))
    );
    assert_eq!(
        singleton.rule.map(|r| r.to_string()).as_deref(),
        Some("903.5b")
    );
    let banned = r
        .violations
        .iter()
        .find(|v| v.code == "banned")
        .unwrap_or_else(|| panic!("banned"));
    assert!(banned.rule.is_none(), "banned lists aren't CR rules");

    let short = check(&deck_text(&["Vexa, Tide Scholar"], &[]).replacen(
        "7 Seven Brothers",
        "6 Seven Brothers",
        1,
    ));
    assert_eq!(codes(&short), vec!["deck_size"]);
}

#[test]
fn basic_land_types_rule() {
    // Murky Bog's identity is colorless (reminder text), but its Swamp type makes B (CR 903.5d).
    let r = check(&deck_text(&["Vexa, Tide Scholar"], &["1 Murky Bog"]));
    assert_eq!(codes(&r), vec!["basic_land_type"]);
}

#[test]
fn partner_variants() {
    let pair = |a: &str, b: &str| codes(&check(&format!("Commander\n1 {a}\n1 {b}\n")));
    let ok = |a: &str, b: &str| {
        let c = pair(a, b);
        assert!(
            !c.contains(&"invalid_partner_pair") && !c.contains(&"ineligible_commander"),
            "{a} + {b}: {c:?}"
        );
    };
    let bad = |a: &str, b: &str| {
        assert!(
            pair(a, b).contains(&"invalid_partner_pair"),
            "{a} + {b} should be rejected"
        );
    };
    ok("Orrin the Bold", "Sela of the Reeds"); // CR 702.124h
    ok("Ana, Ever Loyal", "Bo, Ever Brave"); // CR 702.124i
    ok("Captain Mirelle", "Quill, Deckhand"); // CR 702.124j
    ok("Wandering Archivist", "Raised by Storms"); // CR 702.124k
    ok("Clara Companion", "The Ninth Traveler"); // CR 702.124m
    bad("Orrin the Bold", "Quill, Deckhand"); // CR 702.124f: variants don't mix
    bad("Orrin the Bold", "Ana, Ever Loyal");
    bad("Vexa, Tide Scholar", "Sela of the Reeds");
    let three = codes(&check(
        "Commander\n1 Orrin the Bold\n1 Sela of the Reeds\n1 Vexa, Tide Scholar\n",
    ));
    assert!(three.contains(&"too_many_commanders"));
}

#[test]
fn commander_eligibility() {
    let one =
        |c: &str| codes(&check(&format!("Commander\n1 {c}\n"))).contains(&"ineligible_commander");
    assert!(one("Thicket Guardian"), "not legendary");
    assert!(one("Star Barge"), "Spacecraft without P/T");
    assert!(
        one("Raised by Storms"),
        "Background without choose a Background"
    );
    assert!(!one("Iron Steed"), "legendary Vehicle (CR 903.3)");
    assert!(
        !one("Lorekeeper's Tome"),
        "can be your commander (CR 903.3a)"
    );
}

#[test]
fn unknown_names_get_suggestions() {
    let r = check(&deck_text(&["Vexa, Tide Scholar"], &["1 Thicket Gaurdian"]));
    let v = r
        .violations
        .iter()
        .find(|v| v.code == "unknown_card")
        .unwrap_or_else(|| panic!("{:?}", r.violations));
    assert!(
        v.message.contains("did you mean \"Thicket Guardian\""),
        "{}",
        v.message
    );
    assert!(!r.legal);
}

#[test]
fn explicit_commander_and_sideboard() {
    // Export without a commander section; commander supplied separately and absent from the list.
    let text = deck_text(&["Vexa, Tide Scholar"], &[])
        .replace("Commander\n1 Vexa, Tide Scholar\n", "")
        + "Sideboard\n1 Glimmer Bolt\n";
    let opts = ValidateOptions {
        commanders: vec!["Vexa, Tide Scholar".into()],
        ..Default::default()
    };
    let r = validate(&DB, &parse_text(&text), &opts);
    assert!(r.legal, "{:#?}", r.violations);
    assert!(r.warnings.iter().any(|w| w.code == "ignored_section"
        && w.rule.map(|x| x.to_string()).as_deref() == Some("903.5e")));

    let none = check(&text.replace("Sideboard\n1 Glimmer Bolt\n", ""));
    assert!(codes(&none).contains(&"no_commander"));
}

#[test]
fn unsupported_format() {
    let r = validate(
        &DB,
        &parse_text("1 Brook Sage\n"),
        &ValidateOptions {
            format: "modern".into(),
            ..Default::default()
        },
    );
    assert_eq!(codes(&r), vec!["unsupported_format"]);
}

#[test]
fn report_json_snapshot() {
    let r = check(
        "Commander\n1 Vexa, Tide Scholar\nDeck\n2 Thicket Guardian\n1 Glimmer Bolt\n1 Lorekeepers Tome\n",
    );
    insta::assert_json_snapshot!(r);
}
