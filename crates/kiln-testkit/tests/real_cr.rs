//! Checks against the real Comprehensive Rules, when a local copy exists.
//! The CR is never committed, so these tests skip (with a note) without it.

use std::path::{Path, PathBuf};

use kiln_testkit::RuleTree;
use kiln_types::cr;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load() -> Option<RuleTree> {
    let path = std::env::var_os("KILN_CR_PATH").map_or_else(
        || repo_root().join("docs/_external/cr-lf.txt"),
        PathBuf::from,
    );
    if !path.exists() {
        eprintln!("skipping: no CR text at {}", path.display());
        return None;
    }
    Some(RuleTree::load(&path).unwrap_or_else(|e| panic!("{e}")))
}

#[test]
fn real_cr_shape() {
    let Some(tree) = load() else { return };
    // CR Structure note: ~3,150 numbered rules and subrules.
    assert!(
        (3000..3600).contains(&tree.len()),
        "parsed {} rules",
        tree.len()
    );
    for id in [
        cr!("100"),
        cr!("704.5k"),
        cr!("704.5m"),
        cr!("903.5a"),
        cr!("702.124"),
    ] {
        assert!(tree.contains(id), "missing {id}");
    }
    assert!(
        tree.get(cr!("903.5a"))
            .is_some_and(|r| r.text.contains("100 cards"))
    );
    assert!(
        tree.glossary.len() > 500,
        "glossary has {} entries",
        tree.glossary.len()
    );
}

#[test]
fn vault_and_code_citations_are_valid() {
    let Some(tree) = load() else { return };
    let root = repo_root();
    let (cites, problems) =
        kiln_testkit::check_tree(&tree, &[root.join("docs"), root.join("crates")])
            .unwrap_or_else(|e| panic!("{e}"));
    assert!(cites.len() > 100, "found only {} citations", cites.len());
    let report: Vec<String> = problems
        .iter()
        .map(|p| {
            format!(
                "{}:{} CR {}: {}",
                p.citation.file.display(),
                p.citation.line,
                p.citation.text,
                p.problem
            )
        })
        .collect();
    assert!(
        report.is_empty(),
        "invalid citations:\n{}",
        report.join("\n")
    );
}
