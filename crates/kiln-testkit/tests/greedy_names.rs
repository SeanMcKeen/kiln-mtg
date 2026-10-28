//! The greedy baseline names cards sensibly (CR 201.4): a card still in its
//! library for a reveal-until tutor, or one not in the game when an empty
//! library wins (found by the M8 agent eval: naming blindly decked it).

use std::sync::Arc;

use kiln_engine::CardPool;
use kiln_policy::{GreedyPolicy, Policy};
use kiln_testkit::scenario;

/// Plays `board` up to its name choice and returns greedy's pick, by label.
fn greedy_names(board: &str) -> String {
    let pool = Arc::new(CardPool::m3_test_set());
    let s = scenario::parse(board, "greedy-names").unwrap_or_else(|e| panic!("{e}"));
    let game = scenario::run(&s, &pool).unwrap_or_else(|e| panic!("{e}"));
    let d = game.pending().expect("a pending name choice").clone();
    assert_eq!(scenario::kind_name(&d.kind), "choose_name");
    let i = GreedyPolicy::default().decide(&game.view(d.player), &d);
    game.option_label(i as usize).expect("a label")
}

#[test]
fn names_a_card_in_its_library() {
    let label = greedy_names(
        "scenario: kiln-scenario/1
name: consult for a card that's there
players:
  - battlefield: [Test Swamp]
    hand: [Test Consult]
    library: [Test Forest, Test Bear, Test Ogre]
  - {}
script:
  - choose: Cast Test Consult
  - choose: Pay with Test Swamp
  - pass_all
",
    );
    // A spell before a land; never a name the library lacks.
    assert!(
        label == "Name Test Bear" || label == "Name Test Ogre",
        "{label}"
    );
}

#[test]
fn names_a_missing_card_when_an_empty_library_wins() {
    let label = greedy_names(
        "scenario: kiln-scenario/1
name: consult with a maniac out
players:
  - battlefield: [Test Swamp, Test Maniac]
    hand: [Test Consult]
    library: [Test Forest, Test Bear, Test Ogre]
  - {}
script:
  - choose: Cast Test Consult
  - choose: Pay with Test Swamp
  - pass_all
",
    );
    assert_eq!(label, "Name a card not in this game");
}
