//! Runs every scenario in `tests/scenarios/` against the M2 test card set.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mtg_engine::CardPool;
use mtg_testkit::{RuleTree, scenario};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn files() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(root().join("tests/scenarios"))
        .unwrap_or_else(|e| panic!("{e}"))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "yaml"))
        .collect();
    v.sort();
    v
}

#[test]
fn all_scenarios_pass() {
    let pool = Arc::new(CardPool::test_set());
    let files = files();
    assert!(
        files.len() >= 15,
        "expected the M2 scenario suite, found {}",
        files.len()
    );
    let mut failures = Vec::new();
    for f in &files {
        let s = match scenario::load(f) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}: {e}", f.display()));
                continue;
            }
        };
        if s.cr.is_empty() {
            failures.push(format!("{}: scenario cites no CR rule", f.display()));
        }
        if let Err(e) = scenario::run(&s, &pool) {
            failures.push(format!(
                "{} ({}): {e}",
                f.file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default(),
                s.name
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} scenario(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn scenario_citations_exist() {
    let cr = root().join("docs/_external/cr-lf.txt");
    if !cr.exists() {
        eprintln!("skipping: no CR text");
        return;
    }
    let tree = RuleTree::load(&cr).unwrap_or_else(|e| panic!("{e}"));
    for f in files() {
        let s = scenario::load(&f).unwrap_or_else(|e| panic!("{e}"));
        for r in &s.cr {
            let id = r
                .parse()
                .unwrap_or_else(|e| panic!("{}: {r}: {e}", f.display()));
            assert!(
                tree.contains(id),
                "{}: cites {r}, which isn't in the CR",
                f.display()
            );
        }
    }
}

#[test]
fn wrong_expectations_fail() {
    let pool = Arc::new(CardPool::test_set());
    let path = root().join("tests/scenarios/combat-trample.yaml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{e}"));
    let wrong_life = scenario::parse(
        &text.replace("life: { P2: 17 }", "life: { P2: 16 }"),
        "mutated",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let err = scenario::run(&wrong_life, &pool)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(err.contains("P2 life is 17, expected 16"), "{err}");
    let wrong_options = scenario::parse(
        &text.replace("\"1 damage to P2\"]", "\"1 damage to P1\"]"),
        "mutated",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let err = scenario::run(&wrong_options, &pool)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(
        err.contains("at script step") && err.contains("options are"),
        "{err}"
    );
}

#[test]
fn bad_labels_are_actionable() {
    let pool = Arc::new(CardPool::test_set());
    let s = scenario::parse(
        "scenario: mtg-scenario/1\nname: bad\ncr: [\"117.1\"]\nplayers: [{hand: [Test Bolt]}, {}]\nscript:\n  - choose: Cast Lightning Bolt\n",
        "inline",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let err = scenario::run(&s, &pool)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(
        err.contains("no option with that label") && err.contains("Pass"),
        "{err}"
    );
    let unknown = scenario::parse(
        "scenario: mtg-scenario/1\nname: x\nplayers: [{hand: [Nope]}, {}]\n",
        "inline",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        scenario::run(&unknown, &pool)
            .err()
            .is_some_and(|e| e.to_string().contains("unknown card \"Nope\""))
    );
}
