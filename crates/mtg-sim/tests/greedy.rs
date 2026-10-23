//! The greedy baseline beats random play (M3 plan, B7).
//!
//! Two-player games on the synthetic M3 cards, the same deck pair for both
//! seats, greedy alternating seats; invariants are checked throughout.

use std::sync::Arc;

use mtg_engine::decks::test_deck;
use mtg_engine::rng::game_seed;
use mtg_engine::{CardPool, GameResult, GameRules, GameSetup};
use mtg_policy::{GreedyPolicy, Policy, RandomPolicy};
use mtg_sim::runner::{Checks, run_game};

#[test]
fn greedy_beats_random() {
    let pool = Arc::new(CardPool::m3_test_set());
    let checks = Checks {
        invariants: true,
        views: false,
        replay_every: None,
        concede_per_mille: 0,
    };
    // MTG_GREEDY_GAMES overrides the count (the M3 record used 1,000).
    let games: u64 = std::env::var("MTG_GREEDY_GAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);
    let (mut greedy_wins, mut random_wins, mut other) = (0, 0, 0);
    for i in 0..games {
        let pair = (i / 2) as usize;
        let greedy_seat = (i % 2) as usize;
        let setup = GameSetup {
            rules: GameRules {
                turn_limit: Some(100),
                log_events: false,
                ..GameRules::default()
            },
            decks: vec![test_deck(&pool, pair, 60), test_deck(&pool, pair, 60)],
            seed: game_seed(77, i),
            first_player: None,
            commanders: Vec::new(),
        };
        let mut policies: Vec<Box<dyn Policy>> = (0..2)
            .map(|s| {
                if s == greedy_seat {
                    Box::new(GreedyPolicy) as Box<dyn Policy>
                } else {
                    Box::new(RandomPolicy::default())
                }
            })
            .collect();
        let (outcome, _) = run_game(&pool, setup, &mut policies, &checks, false);
        assert!(
            outcome.violation.is_none(),
            "game {i}: {:?}",
            outcome.violation
        );
        assert!(outcome.panic.is_none(), "game {i}: {:?}", outcome.panic);
        match outcome.result {
            Some(GameResult::Win { player, .. }) if player.0 as usize == greedy_seat => {
                greedy_wins += 1;
            }
            Some(GameResult::Win { .. }) => random_wins += 1,
            _ => other += 1,
        }
    }
    eprintln!("greedy {greedy_wins}, random {random_wins}, other {other} of {games}");
    assert!(
        greedy_wins > 2 * random_wins,
        "greedy {greedy_wins} vs random {random_wins}"
    );
}
