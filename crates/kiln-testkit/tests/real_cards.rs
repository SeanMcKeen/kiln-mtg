//! Real-card verification (M3 plan, B8). Runs only with a local card DB
//! (`KILN_CARD_DB` or `data/cache/current`); otherwise it skips with a note.

use kiln_testkit::real::{locate_db, verify};

#[test]
fn real_card_scenarios_verify_every_supported_definition() {
    let Some(db) = locate_db() else {
        eprintln!("skipping: no card DB (set KILN_CARD_DB or run `kiln data build`)");
        return;
    };
    let report = verify(&db).unwrap_or_else(|e| panic!("{e}"));
    eprintln!(
        "{} definitions: {} verified, {} unsupported, {} unverified",
        report.definitions,
        report.verified.len(),
        report.unsupported.len(),
        report.unverified.len()
    );
    for r in &report.unsupported {
        eprintln!("  unsupported: {}: {}", r.card, r.reason);
    }
    assert!(
        report.failures.is_empty(),
        "{} real-card scenario(s) failed:\n{}",
        report.failures.len(),
        report
            .failures
            .iter()
            .map(|(f, e)| format!("{f}: {e}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        report.unverified.is_empty(),
        "supported definitions without a passing scenario: {:?}",
        report.unverified
    );
}
