//! Card definition files load, validate and round-trip.

use std::path::Path;

use mtg_cards::defs::{CardFile, load_dir, parse};

fn defs_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cards/defs")
}

#[test]
fn all_definitions_load_and_round_trip() {
    let defs = load_dir(&defs_dir()).unwrap_or_else(|e| panic!("{e}"));
    assert!(defs.len() >= 20, "found {} definitions", defs.len());
    for d in &defs {
        let yaml = yaml_serde::to_string(d).unwrap_or_else(|e| panic!("{}: {e}", d.card));
        let back: CardFile =
            parse(&yaml, &d.card).unwrap_or_else(|e| panic!("{}: {e}\n{yaml}", d.card));
        assert_eq!(&back, d, "{} doesn't round-trip", d.card);
    }
    let escapes: Vec<(String, Vec<String>)> = defs
        .iter()
        .map(|d| (d.card.clone(), d.ir().escapes()))
        .filter(|(_, e)| !e.is_empty())
        .collect();
    eprintln!("{} definitions, escape hatches: {escapes:?}", defs.len());
}

#[test]
fn invalid_target_reference_is_rejected() {
    let bad = "card: Bad\nfaces:\n  - abilities:\n      - spell:\n          effect: { counter: { one: { target: 0 } } }\n";
    let err = parse(bad, "bad")
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(err.contains("refers to target 0 but declares 0"), "{err}");
}

#[test]
fn unknown_fields_are_rejected() {
    let bad = "card: Bad\nfaces:\n  - abilities:\n      - spell:\n          effect: { draw: { who: you, amount: 1, extra: 2 } }\n";
    assert!(parse(bad, "bad").is_err());
}
