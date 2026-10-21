//! The card library's precedence (ADR-0007, M3 plan C1). Needs the local
//! card DB; skips without one.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use mtg_cards::{Coverage, Provenance};
use mtg_data::{CardDb, NameMatch};
use mtg_sim::library::Library;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn db() -> Option<CardDb> {
    let explicit = std::env::var_os("MTG_CARD_DB").map(PathBuf::from);
    let path = CardDb::locate(explicit.as_deref(), &root().join("data/cache")).ok()?;
    CardDb::load(&path).ok()
}

#[test]
fn overrides_win_parser_fills_in_and_gaps_are_named() {
    let Some(db) = db() else {
        eprintln!("skipping: no card DB");
        return;
    };
    let defs =
        mtg_cards::defs::load_dir(&root().join("cards/defs")).unwrap_or_else(|e| panic!("{e}"));
    let verified: BTreeSet<String> = ["Sol Ring".to_owned()].into();
    let lib = Library::build(&db, &defs, &verified);
    let card = |name: &str| {
        let NameMatch::Exact(i) = db.resolve(name, "commander") else {
            panic!("{name} not in the DB");
        };
        lib.get(db.cards()[i].oracle_id)
            .unwrap_or_else(|| panic!("{name} not in the library"))
    };

    let sol = card("Sol Ring");
    assert!(matches!(
        sol.definition.provenance,
        Provenance::Override { .. }
    ));
    assert!(sol.definition.coverage.is_verified());
    assert!(sol.def.is_some());

    // A definition without a passing scenario is only parsed.
    let murder = card("Murder");
    assert_eq!(murder.definition.coverage, Coverage::Parsed);

    // A vanilla creature comes from the parser (no override exists).
    let giant = card("Hill Giant");
    assert!(
        matches!(giant.definition.provenance, Provenance::Parser { .. }),
        "{:?}",
        giant.definition
    );
    assert_eq!(giant.definition.coverage, Coverage::Parsed);

    // A triggered ability keeps a card unsupported, and the reason says why.
    let study = card("Rhystic Study");
    let Coverage::Unsupported { reasons } = &study.definition.coverage else {
        panic!("{:?}", study.definition.coverage);
    };
    assert!(
        reasons.iter().any(|r| r.reason.contains("M4")),
        "{reasons:?}"
    );
    assert!(study.def.is_none());
}
