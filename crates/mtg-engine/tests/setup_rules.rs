//! Game-start rules (CR 103) that scenario files can't reach, because
//! scenarios begin mid-game.

use std::sync::Arc;

use mtg_core::PlayerId;
use mtg_engine::decks::test_deck;
use mtg_engine::{CardPool, DecisionKind, Game, GameRules, GameSetup, Step};

fn new_game(players: usize) -> Game {
    let pool = Arc::new(CardPool::test_set());
    let decks = (0..players).map(|i| test_deck(&pool, i, 60)).collect();
    let setup = GameSetup {
        rules: GameRules::default(),
        decks,
        seed: 11,
        first_player: Some(PlayerId(0)),
    };
    Game::new(pool, &setup).unwrap_or_else(|e| panic!("{e}"))
}

/// Answers option 0 (keep, pass, …) until the starting player's first main phase.
fn to_first_main(game: &mut Game) {
    while !(game.turn().number == 1 && game.turn().step == Step::PrecombatMain) {
        game.apply_option(0).unwrap_or_else(|e| panic!("{e}"));
    }
}

#[test]
fn starting_player_skips_first_draw_only_in_two_player_games() {
    // CR 103.8a: two players → the starting player skips their first draw step.
    let mut two = new_game(2);
    to_first_main(&mut two);
    assert_eq!(two.player_state(PlayerId(0)).hand.len(), 7);
    // CR 103.8c: in other multiplayer games, nobody skips it.
    let mut four = new_game(4);
    to_first_main(&mut four);
    assert_eq!(four.player_state(PlayerId(0)).hand.len(), 8);
}

#[test]
fn first_mulligan_is_free_in_multiplayer() {
    // CR 103.5c: in a multiplayer game the first mulligan doesn't count.
    let mut game = new_game(4);
    let d = game
        .pending()
        .cloned()
        .unwrap_or_else(|| panic!("decision"));
    assert_eq!(
        (d.player, &d.kind),
        (PlayerId(0), &DecisionKind::Mulligan { mulligans_taken: 0 })
    );
    game.apply_option(1).unwrap_or_else(|e| panic!("{e}")); // P1 mulligans
    for _ in 0..3 {
        game.apply_option(0).unwrap_or_else(|e| panic!("{e}")); // others keep
    }
    // No cards to bottom: the next decision is P1's second declaration.
    let d = game
        .pending()
        .cloned()
        .unwrap_or_else(|| panic!("decision"));
    assert_eq!(
        (d.player, &d.kind),
        (PlayerId(0), &DecisionKind::Mulligan { mulligans_taken: 1 })
    );
    assert_eq!(game.player_state(PlayerId(0)).hand.len(), 7);
    game.apply_option(1).unwrap_or_else(|e| panic!("{e}"));
    // Second mulligan: bottom one card (CR 103.5).
    let d = game
        .pending()
        .cloned()
        .unwrap_or_else(|| panic!("decision"));
    assert!(
        matches!(d.kind, DecisionKind::BottomCard { remaining: 1, .. }),
        "{d:?}"
    );
    game.apply_option(0).unwrap_or_else(|e| panic!("{e}"));
    game.apply_option(0).unwrap_or_else(|e| panic!("{e}")); // keep
    assert_eq!(game.player_state(PlayerId(0)).hand.len(), 6);
    assert_eq!(game.player_state(PlayerId(0)).library.len(), 60 - 6);
}

#[test]
fn two_player_mulligans_cost_a_card_immediately() {
    let mut game = new_game(2);
    game.apply_option(1).unwrap_or_else(|e| panic!("{e}")); // P1 mulligans
    game.apply_option(0).unwrap_or_else(|e| panic!("{e}")); // P2 keeps
    let d = game
        .pending()
        .cloned()
        .unwrap_or_else(|| panic!("decision"));
    assert!(
        matches!(d.kind, DecisionKind::BottomCard { remaining: 1, .. }),
        "{d:?}"
    );
}

#[test]
fn setup_rejects_bad_input() {
    let pool = Arc::new(CardPool::test_set());
    let one = GameSetup {
        rules: GameRules::default(),
        decks: vec![vec![]],
        seed: 0,
        first_player: None,
    };
    assert!(Game::new(Arc::clone(&pool), &one).is_err());
    let bad_def = GameSetup {
        rules: GameRules::default(),
        decks: vec![vec![mtg_engine::DefId(9999)], vec![]],
        seed: 0,
        first_player: None,
    };
    assert!(Game::new(Arc::clone(&pool), &bad_def).is_err());
    let bad_first = GameSetup {
        rules: GameRules::default(),
        decks: vec![vec![], vec![]],
        seed: 0,
        first_player: Some(PlayerId(5)),
    };
    assert!(Game::new(pool, &bad_first).is_err());
}
