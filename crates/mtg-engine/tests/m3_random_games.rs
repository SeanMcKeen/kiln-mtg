//! Random play over the M3 synthetic IR cards: invariants and view audits
//! after every decision, "every offered option applies", replay
//! determinism, and coverage of the interpreter's decision kinds.

use std::collections::BTreeMap;
use std::sync::Arc;

use mtg_core::PlayerId;
use mtg_engine::decks::test_deck;
use mtg_engine::rng::GameRng;
use mtg_engine::{CardPool, DecisionKind, Game, GameRules, GameSetup};

fn setup(players: usize, seed: u64) -> (Arc<CardPool>, GameSetup) {
    let pool = Arc::new(CardPool::m3_test_set());
    let decks = (0..players)
        .map(|i| test_deck(&pool, i + seed as usize, 60))
        .collect();
    let rules = GameRules {
        turn_limit: Some(150),
        ..GameRules::default()
    };
    (
        pool,
        GameSetup {
            rules,
            decks,
            seed,
            first_player: None,
            commanders: Vec::new(),
        },
    )
}

/// Uniform random choice, passing half the time at priority so games move.
fn pick(rng: &mut GameRng, game: &Game) -> u32 {
    let d = game.pending().unwrap_or_else(|| panic!("no decision"));
    if matches!(d.kind, DecisionKind::Priority { .. }) && rng.below(2) == 0 {
        return 0;
    }
    u32::try_from(rng.index(d.option_count())).unwrap_or(0)
}

fn kind(k: &DecisionKind) -> &'static str {
    match k {
        DecisionKind::Optional { .. } => "optional",
        DecisionKind::ChooseCard { .. } => "choose_card",
        DecisionKind::ChooseMana { .. } => "choose_mana",
        DecisionKind::ChooseName { .. } => "choose_name",
        DecisionKind::ChooseTarget { .. } => "choose_target",
        DecisionKind::ChooseMode { .. } => "choose_mode",
        DecisionKind::ChooseCost { .. } => "choose_cost",
        DecisionKind::ChooseSymbolPay { .. } => "choose_symbol_pay",
        DecisionKind::ChooseNumber { .. } => "choose_number",
        _ => "other",
    }
}

fn play(players: usize, seed: u64, counts: &mut BTreeMap<&'static str, u32>) -> (Game, Vec<u32>) {
    let (pool, setup) = setup(players, seed);
    let sizes: Vec<usize> = setup.decks.iter().map(Vec::len).collect();
    let mut game = Game::new(pool, &setup).unwrap_or_else(|e| panic!("{e}"));
    let mut rng = GameRng::new(seed ^ 0x5EED);
    let mut choices = Vec::new();
    while !game.is_over() {
        if let Some(d) = game.pending() {
            *counts.entry(kind(&d.kind)).or_default() += 1;
        }
        let c = pick(&mut rng, &game);
        choices.push(c);
        game.apply_option(c)
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        game.check_invariants(&sizes)
            .unwrap_or_else(|e| panic!("seed {seed} after {} choices: {e}", choices.len()));
        for p in 0..players {
            game.audit_view(PlayerId(u8::try_from(p).unwrap_or(0)))
                .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        }
        assert!(choices.len() < 200_000, "seed {seed}: runaway game");
    }
    (game, choices)
}

#[test]
fn m3_games_keep_invariants_and_reach_every_decision_kind() {
    let mut counts = BTreeMap::new();
    for seed in 0..30 {
        play(4, seed, &mut counts);
    }
    eprintln!("{counts:?}");
    for k in [
        "optional",
        "choose_card",
        "choose_mana",
        "choose_name",
        "choose_target",
        "choose_mode",
        // `choose_cost` needs both costs payable at once, which random decks
        // rarely produce; tests/scenarios/cast-choose-cost.yaml covers it.
        "choose_symbol_pay",
        "choose_number",
    ] {
        assert!(
            counts.get(k).copied().unwrap_or(0) > 0,
            "{k} never offered: {counts:?}"
        );
    }
}

#[test]
fn m3_replays_reproduce() {
    let mut counts = BTreeMap::new();
    for seed in 0..6 {
        let (game, choices) = play(3, seed + 100, &mut counts);
        let (pool, setup) = setup(3, seed + 100);
        let mut again = Game::new(pool, &setup).unwrap_or_else(|e| panic!("{e}"));
        for c in choices {
            again.apply_option(c).unwrap_or_else(|e| panic!("{e}"));
        }
        assert_eq!(again.state_hash(), game.state_hash(), "seed {seed}");
    }
}

#[test]
fn m3_every_offered_option_applies() {
    let (pool, setup) = setup(4, 11);
    let sizes: Vec<usize> = setup.decks.iter().map(Vec::len).collect();
    let mut game = Game::new(pool, &setup).unwrap_or_else(|e| panic!("{e}"));
    let mut rng = GameRng::new(5);
    let mut decisions = 0;
    while !game.is_over() && decisions < 4_000 {
        let n = game
            .pending()
            .map_or(0, mtg_engine::PendingDecision::option_count);
        for i in 0..n {
            let mut g = game.clone();
            g.apply_option(u32::try_from(i).unwrap_or(0))
                .unwrap_or_else(|e| panic!("decision {decisions} option {i}: {e}"));
            g.check_invariants(&sizes)
                .unwrap_or_else(|e| panic!("decision {decisions} option {i}: {e}"));
        }
        let c = pick(&mut rng, &game);
        game.apply_option(c).unwrap_or_else(|e| panic!("{e}"));
        decisions += 1;
    }
}
