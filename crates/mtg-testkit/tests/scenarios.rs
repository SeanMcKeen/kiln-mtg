//! Runs every scenario in `tests/scenarios/` against the synthetic test cards (M2 set plus M3 IR cards).

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
    let pool = Arc::new(CardPool::m3_test_set());
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
    // Real-card scenarios need the card DB to run, but their citations can always be checked.
    let mut real: Vec<PathBuf> = std::fs::read_dir(root().join("tests/scenarios-real"))
        .map(|d| {
            d.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "yaml"))
                .collect()
        })
        .unwrap_or_default();
    real.sort();
    assert!(
        real.len() >= 100,
        "expected the real-card scenarios, found {}",
        real.len()
    );
    for f in files().into_iter().chain(real) {
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
    let pool = Arc::new(CardPool::m3_test_set());
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
    let pool = Arc::new(CardPool::m3_test_set());
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

/// CR 406.3: a card exiled face down is in no player's view.
#[test]
fn face_down_exile_is_hidden_from_every_view() {
    let pool = Arc::new(CardPool::m3_test_set());
    let s = scenario::parse(
        r#"
scenario: mtg-scenario/1
name: face-down exile
cr: ["406.3"]
players:
  - battlefield: [Test Swamp]
    hand: [Test Hoard Seer]
  - {}
script:
  - choose: Cast Test Hoard Seer
  - choose: Pay with Test Swamp
  - pass_all
expect:
  zones: { exile: [Test Wastes] }
"#,
        "face-down",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let game = scenario::run(&s, &pool).unwrap_or_else(|e| panic!("{e}"));
    for p in [mtg_core::PlayerId(0), mtg_core::PlayerId(1)] {
        assert!(
            game.view(p).snapshot().exile.is_empty(),
            "{p:?} can see a face-down exiled card"
        );
    }
}

/// M4 exit criterion: the Isochron/Reversal loop runs as a shortcut of 10^9
/// iterations in under 1 ms (CR 732.2c; D9, D10). Wall-clock time is used
/// only to measure the test, never by the engine.
#[test]
fn scepter_reversal_shortcut_is_fast() {
    let pool = Arc::new(CardPool::m3_test_set());
    let text = std::fs::read_to_string(root().join("tests/scenarios/loop-shortcut.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    // Play the scenario up to the shortcut proposal.
    let cut = text
        .find("  - choose: Repeat the loop 1000 times")
        .unwrap_or_else(|| panic!("scenario changed"));
    let mut s = scenario::parse(&text[..cut], "shortcut").unwrap_or_else(|e| panic!("{e}"));
    s.expect = scenario::Expect::default();
    let mut game = scenario::run(&s, &pool).unwrap_or_else(|e| panic!("{e}"));
    let p1 = mtg_core::PlayerId(0);
    let options = |g: &mtg_engine::Game| {
        g.pending()
            .map(mtg_engine::PendingDecision::option_count)
            .unwrap_or_else(|| panic!("no decision"))
    };
    let start = std::time::Instant::now();
    // The largest shortcut on offer (10^9), then the opponent accepts.
    let n = options(&game);
    game.apply_option(u32::try_from(n - 1).unwrap_or(0))
        .unwrap_or_else(|e| panic!("{e:?}"));
    game.apply_option(0).unwrap_or_else(|e| panic!("{e:?}"));
    let elapsed = start.elapsed();
    let pool_c = game.view(p1).snapshot().players[0].mana_pool[5];
    assert_eq!(
        pool_c,
        6 + 2 * 1_000_000_000,
        "colorless mana after the shortcut"
    );
    // Debug builds are much slower than release; the 1 ms bound is for release.
    let bound = if cfg!(debug_assertions) { 50 } else { 1 };
    assert!(
        elapsed.as_millis() < bound,
        "shortcut took {elapsed:?} (bound {bound} ms)"
    );
    eprintln!("10^9-iteration shortcut applied in {elapsed:?}");
}
