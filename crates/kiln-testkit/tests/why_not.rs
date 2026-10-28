//! `Game::why_not` (FR-E10, M10 plan L1): why an action isn't offered, with
//! the rule and the card responsible.

use std::sync::Arc;

use kiln_engine::{CardPool, Game};
use kiln_testkit::scenario;
use kiln_types::PlayerId;

fn board(text: &str) -> Game {
    let pool = Arc::new(CardPool::m3_test_set());
    let s = scenario::parse(text, "why-not").unwrap_or_else(|e| panic!("{e}"));
    scenario::run(&s, &pool).unwrap_or_else(|e| panic!("{e}"))
}

/// The rules cited for why `p` can't do `what`.
fn why(game: &Game, p: u8, what: &str) -> Vec<String> {
    let action = game
        .action_for(PlayerId(p), what)
        .unwrap_or_else(|| panic!("no action for {what:?}"));
    game.why_not(PlayerId(p), &action)
        .into_iter()
        .map(|e| format!("{} {}", e.cr, e.reason))
        .collect()
}

#[test]
fn a_sorcery_on_an_opponents_turn() {
    let g = board(
        "scenario: kiln-scenario/1
name: sorcery timing
players:
  - battlefield: [Test Mountain, Test Mountain, Test Mountain]
  - battlefield: [Test Mountain, Test Mountain, Test Mountain]
    hand: [Test Blast, Test Bolt]
turn: { number: 3, active: P1, step: precombat_main }
script:
  - pass: 1
",
    );
    let r = why(&g, 1, "Cast Test Blast");
    assert_eq!(r.len(), 1, "{r:?}");
    assert!(
        r[0].starts_with("117.1a") && r[0].contains("isn't this player's turn"),
        "{r:?}"
    );
    assert!(
        why(&g, 1, "Cast Test Bolt").is_empty(),
        "an instant is fine"
    );
}

#[test]
fn no_target_no_mana_and_a_second_land() {
    let g = board(
        "scenario: kiln-scenario/1
name: targets, costs, lands
players:
  - battlefield: [Test Swamp]
    hand: [Test Murder, Test Forest]
  - {}
turn: { number: 3, active: P1, step: precombat_main }
script:
  - choose: Play Test Forest
",
    );
    let r = why(&g, 0, "Cast Test Murder");
    assert!(r.iter().any(|e| e.starts_with("601.2c")), "{r:?}");
    let g2 = board(
        "scenario: kiln-scenario/1
name: costs
players:
  - battlefield: [Test Swamp, Test Bear]
    hand: [Test Murder]
  - {}
turn: { number: 3, active: P1, step: precombat_main }
script: []
",
    );
    let r = why(&g2, 0, "Test Murder");
    assert_eq!(r.len(), 1, "{r:?}");
    assert!(r[0].starts_with("601.2h"), "{r:?}");
    let g3 = board(
        "scenario: kiln-scenario/1
name: lands
players:
  - hand: [Test Forest, Test Swamp]
  - {}
turn: { number: 3, active: P1, step: precombat_main }
script:
  - choose: Play Test Forest
",
    );
    let r = why(&g3, 0, "Play Test Swamp");
    assert_eq!(r.len(), 1, "{r:?}");
    assert!(r[0].starts_with("305.2"), "{r:?}");
}

#[test]
fn a_cant_cast_static_is_named() {
    let g = board(
        "scenario: kiln-scenario/1
name: abolisher
players:
  - battlefield: [Test Abolisher]
  - battlefield: [Test Mountain]
    hand: [Test Bolt]
turn: { number: 3, active: P1, step: precombat_main }
script:
  - pass: 1
",
    );
    let r = why(&g, 1, "Cast Test Bolt");
    assert!(
        r.iter()
            .any(|e| e.starts_with("101.2") && e.contains("Test Abolisher")),
        "{r:?}"
    );
}

#[test]
fn without_priority() {
    let g = board(
        "scenario: kiln-scenario/1
name: no priority
players:
  - battlefield: [Test Mountain]
  - battlefield: [Test Mountain]
    hand: [Test Bolt]
turn: { number: 3, active: P1, step: precombat_main }
script: []
",
    );
    let r = why(&g, 1, "Cast Test Bolt");
    assert!(r.iter().any(|e| e.starts_with("117.1 ")), "{r:?}");
}
