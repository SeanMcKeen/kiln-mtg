//! Golden replays (`docs/08 Testing/Golden Replays.md`, NFR-C3).
//!
//! Recorded random games are committed under `tests/golden/`. Replaying one
//! must reproduce every decision and the final state hash. A change in engine
//! behaviour fails here and must be re-blessed deliberately:
//!
//! ```sh
//! MTG_BLESS=1 cargo test -p mtg-sim --test golden   # then explain why in the commit
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mtg_engine::decks::test_deck;
use mtg_engine::{CardPool, GameRules, GameSetup, Replay};
use mtg_policy::{Policy, RandomPolicy};
use mtg_sim::runner::{Checks, run_game};

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden")
}

/// The recorded games: (file stem, players, seed).
const GAMES: [(&str, usize, u64); 3] = [
    ("m2-random-2p", 2, 7),
    ("m2-random-4p", 4, 2026),
    ("m2-random-6p", 6, 99),
];

fn record(players: usize, seed: u64) -> Replay {
    let pool = Arc::new(CardPool::test_set());
    let decks = (0..players).map(|i| test_deck(&pool, i, 60)).collect();
    let setup = GameSetup {
        rules: GameRules {
            log_events: false,
            turn_limit: Some(300),
            ..GameRules::default()
        },
        decks,
        seed,
        first_player: None,
    };
    let mut policies: Vec<Box<dyn Policy>> = (0..players)
        .map(|_| Box::new(RandomPolicy::new(600)) as Box<dyn Policy>)
        .collect();
    let (outcome, replay) = run_game(&pool, setup, &mut policies, &Checks::default(), true);
    assert!(outcome.violation.is_none(), "{:?}", outcome.violation);
    replay.unwrap_or_else(|| panic!("recording requested"))
}

#[test]
fn golden_replays_reproduce() {
    let bless = std::env::var_os("MTG_BLESS").is_some();
    let pool = Arc::new(CardPool::test_set());
    for (stem, players, seed) in GAMES {
        let path = golden_dir().join(format!("{stem}.json"));
        if bless {
            let replay = record(players, seed);
            std::fs::create_dir_all(golden_dir()).unwrap_or_else(|e| panic!("{e}"));
            std::fs::write(&path, serde_json::to_string(&replay).unwrap_or_default())
                .unwrap_or_else(|e| panic!("{e}"));
            eprintln!(
                "blessed {} ({} choices)",
                path.display(),
                replay.choices.len()
            );
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e}; bless with MTG_BLESS=1", path.display()));
        let replay: Replay =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let game = replay.run(Arc::clone(&pool)).unwrap_or_else(|e| panic!("{stem}: {e}. If this change is intentional, re-bless with MTG_BLESS=1 and say why in the commit."));
        assert_eq!(game.result().copied(), replay.result, "{stem}");
    }
}
