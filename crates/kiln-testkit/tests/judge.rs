//! The judge session (M10 plan L3): why-not, choosing, why, branches.

use std::sync::Arc;

use kiln_engine::CardPool;
use kiln_testkit::judge::Judge;
use kiln_testkit::scenario;

#[test]
fn a_judge_session() {
    let pool = Arc::new(CardPool::m3_test_set());
    let s = scenario::parse(
        "scenario: kiln-scenario/1
name: judge
players:
  - battlefield: [Test Mountain]
    hand: [Test Bolt, Test Blast]
  - life: 3
turn: { number: 3, active: P1, step: precombat_main }
script: []
",
        "judge",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let mut j = Judge::load(&s, &pool).unwrap_or_else(|e| panic!("{e}"));
    let r = j
        .command("why-not Test Blast")
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(r.contains("CR 601.2h"), "{r}");
    j.command("branch").unwrap_or_else(|e| panic!("{e}"));
    for c in ["Cast Test Bolt", "Target P2", "0", "Pass", "Pass"] {
        j.command(c).unwrap_or_else(|e| panic!("{c}: {e}"));
    }
    assert_eq!(
        j.command("result").ok().as_deref(),
        Some("P1 wins on turn 3")
    );
    let lost = j
        .game()
        .describe_events()
        .iter()
        .rposition(|e| e.text.contains("loses the game"))
        .unwrap_or_else(|| panic!("no loss event"));
    let r = j
        .command(&format!("why {lost}"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(r.contains("because of"), "{r}");
    j.command("back").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(j.command("result").ok().as_deref(), Some("in progress"));
}
