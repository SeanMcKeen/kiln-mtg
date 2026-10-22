//! Engine benchmarks (`docs/03 Architecture/Performance & Cheap Cloning.md`).
//!
//! 1. `GameState` clone at a busy 4-player board (NFR-P2: ≤ 10 µs)
//! 2. Legal-option enumeration at priority (NFR-P3: ≤ 20 µs)
//! 3. A complete random 4-player game (NFR-P1: ≤ 5 ms engine time per game)
//! 4. All characteristics of a 60-permanent board with 10 static abilities,
//!    recomputed from scratch (M5 plan L6: ≤ 20 µs)

use std::hint::black_box;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use mtg_core::PlayerId;
use mtg_engine::decks::test_deck;
use mtg_engine::rng::GameRng;
use mtg_engine::{
    CardPool, DecisionKind, Game, GameBuilder, GameRules, GameSetup, PermanentOpts, ZoneId,
};

/// 4 players × 15 permanents: anthems, a land-type setter and adder, a
/// 613.6 pair, rule statics, creatures (some with counters) and lands.
fn layered_board() -> Game {
    let pool = Arc::new(CardPool::m3_test_set());
    let id = |n: &str| pool.by_name(n).unwrap_or_else(|| panic!("{n}"));
    let statics = [
        "Test Anthem",
        "Test Moon",
        "Test Crypt",
        "Test Humbler",
        "Test Opal",
        "Test Anthem",
        "Test Orb",
        "Test Abolisher",
        "Test Anthem",
        "Test Anthem",
    ];
    let mut b = GameBuilder::new(Arc::clone(&pool), 4);
    let mut placed = 0;
    for p in 0..4u8 {
        let who = PlayerId(p);
        for i in 0..15 {
            let name = if placed < statics.len() && i < 3 {
                statics[placed]
            } else if i % 3 == 0 {
                "Test Confluence"
            } else if i % 3 == 1 {
                "Test Bear"
            } else {
                "Test Island"
            };
            if placed < statics.len() && i < 3 {
                placed += 1;
            }
            b = b.permanent(who, id(name), PermanentOpts::default());
        }
    }
    b.build()
}

fn setup(seed: u64) -> (Arc<CardPool>, GameSetup) {
    let pool = Arc::new(CardPool::test_set());
    let decks = (0..4).map(|i| test_deck(&pool, i, 60)).collect();
    let rules = GameRules {
        log_events: false,
        ..GameRules::default()
    };
    (
        pool,
        GameSetup {
            rules,
            decks,
            seed,
            first_player: None,
        },
    )
}

fn random_step(game: &mut Game, rng: &mut GameRng) {
    let Some(d) = game.pending() else { return };
    let n = d.option_count();
    let pass = matches!(d.kind, DecisionKind::Priority { .. }) && rng.below(2) == 0;
    let c = if pass { 0 } else { rng.index(n) };
    let _ = game.apply_option(u32::try_from(c).unwrap_or(0));
}

/// A mid-game board: the busiest priority decision before turn 40.
fn mid_game() -> Game {
    let (pool, setup) = setup(3);
    let mut game = Game::new(pool, &setup).unwrap_or_else(|e| panic!("{e}"));
    let mut rng = GameRng::new(1);
    let mut best = game.clone();
    let mut best_size = 0;
    while !game.is_over() && game.turn().number < 40 {
        random_step(&mut game, &mut rng);
        if matches!(
            game.pending().map(|d| &d.kind),
            Some(DecisionKind::Priority { .. })
        ) {
            let size = game.objects_in(ZoneId::Battlefield).len();
            if size > best_size {
                best_size = size;
                best = game.clone();
            }
        }
    }
    best
}

fn benches(c: &mut Criterion) {
    let board = mid_game();
    let perms = board.objects_in(ZoneId::Battlefield).len();
    let holder = board
        .pending()
        .map(|d| d.player)
        .unwrap_or_else(|| panic!("decision"));
    eprintln!(
        "mid-game board: {perms} permanents, turn {}",
        board.turn().number
    );

    c.bench_function(&format!("clone 4p mid-game ({perms} permanents)"), |b| {
        b.iter(|| black_box(board.clone()));
    });
    c.bench_function("priority enumeration (mid-game)", |b| {
        b.iter(|| black_box(board.priority_actions(holder)));
    });
    c.bench_function("state hash (mid-game)", |b| {
        b.iter(|| black_box(board.state_hash()));
    });

    let mut layered = layered_board();
    let n = layered.objects_in(ZoneId::Battlefield).len();
    c.bench_function(
        &format!("characteristics, recomputed ({n} permanents, 10 statics)"),
        |b| b.iter(|| black_box(layered.bench_characteristics())),
    );

    let mut seed = 0u64;
    c.bench_function("random 4p game (synthetic decks)", |b| {
        b.iter(|| {
            seed += 1;
            let (pool, setup) = setup(seed);
            let mut game = Game::new(pool, &setup).unwrap_or_else(|e| panic!("{e}"));
            let mut rng = GameRng::new(seed);
            while !game.is_over() {
                random_step(&mut game, &mut rng);
            }
            black_box(game.turn().number)
        });
    });
}

criterion_group! {
    name = engine;
    config = Criterion::default().sample_size(30);
    targets = benches
}
criterion_main!(engine);
