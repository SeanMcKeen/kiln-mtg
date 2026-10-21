//! Random-play smoke tests: complete games with invariants checked after
//! every decision, determinism, and "every offered option applies".

use std::sync::Arc;

use mtg_core::PlayerId;
use mtg_engine::decks::test_deck;
use mtg_engine::rng::GameRng;
use mtg_engine::{CardPool, DecisionKind, Game, GameResult, GameRules, GameSetup};

fn setup(players: usize, seed: u64) -> (Arc<CardPool>, GameSetup) {
    let pool = Arc::new(CardPool::test_set());
    let decks = (0..players)
        .map(|i| test_deck(&pool, i + seed as usize, 60))
        .collect();
    let rules = GameRules {
        turn_limit: Some(200),
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

/// Uniform random choice, passing half the time at priority so games move.
fn pick(rng: &mut GameRng, game: &Game) -> u32 {
    let d = game.pending().unwrap_or_else(|| panic!("no decision"));
    let n = d.option_count();
    if matches!(d.kind, DecisionKind::Priority { .. }) && rng.below(2) == 0 {
        return 0;
    }
    u32::try_from(rng.index(n)).unwrap_or(0)
}

fn play(players: usize, seed: u64, check: bool) -> (Game, Vec<u32>) {
    let (pool, setup) = setup(players, seed);
    let sizes: Vec<usize> = setup.decks.iter().map(Vec::len).collect();
    let mut game = Game::new(pool, &setup).unwrap_or_else(|e| panic!("{e}"));
    let mut rng = GameRng::new(seed ^ 0xABCD);
    let mut choices = Vec::new();
    let mut steps = 0;
    while !game.is_over() {
        let c = pick(&mut rng, &game);
        choices.push(c);
        game.apply_option(c)
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        if check {
            game.check_invariants(&sizes)
                .unwrap_or_else(|e| panic!("seed {seed} after {steps} choices: {e}"));
        }
        steps += 1;
        assert!(steps < 200_000, "seed {seed}: runaway game");
    }
    (game, choices)
}

#[test]
fn random_four_player_games_finish_with_invariants() {
    let mut wins = 0;
    for seed in 0..40 {
        let (game, _) = play(4, seed, true);
        if matches!(game.result(), Some(GameResult::Win { .. })) {
            wins += 1;
        }
    }
    assert!(wins > 0, "no game produced a winner");
}

#[test]
fn random_games_exercise_the_rules() {
    use mtg_engine::Event;
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    let mut results: BTreeMap<String, u32> = BTreeMap::new();
    let mut turns = 0;
    let games = 60;
    for seed in 0..games {
        let (pool, setup) = setup(4, seed);
        let mut game = Game::new(pool, &setup).unwrap_or_else(|e| panic!("{e}"));
        let mut rng = GameRng::new(seed ^ 0xABCD);
        while !game.is_over() {
            if let Some(d) = game.pending() {
                let key = match d.kind {
                    DecisionKind::AssignDamage { .. } => "decision:assign_damage",
                    DecisionKind::LegendRule { .. } => "decision:legend_rule",
                    DecisionKind::BottomCard { .. } => "decision:bottom_card",
                    DecisionKind::Discard { .. } => "decision:discard",
                    DecisionKind::ChooseTarget { .. } => "decision:target",
                    DecisionKind::PayMana { .. } => "decision:pay_mana",
                    DecisionKind::ManaStep { .. } => "decision:mana_step",
                    DecisionKind::DeclareBlocker { .. } => "decision:block",
                    _ => "decision:other",
                };
                *counts.entry(key).or_default() += 1;
            }
            let c = pick(&mut rng, &game);
            game.apply_option(c).unwrap_or_else(|e| panic!("{e}"));
        }
        for e in game.events() {
            let key = match e {
                Event::SpellCast { .. } => "spell_cast",
                Event::DidNotResolve { .. } => "did_not_resolve",
                Event::AttackerDeclared { .. } => "attack",
                Event::BlockerDeclared { .. } => "block",
                Event::Damage { combat: true, .. } => "combat_damage",
                Event::Damage { combat: false, .. } => "spell_damage",
                Event::Destroyed { .. } => "destroyed",
                Event::LifeGained { .. } => "life_gained",
                Event::LandPlayed { .. } => "land_played",
                Event::Mulligan { .. } => "mulligan",
                Event::PlayerLost { reason, .. } => match reason {
                    mtg_engine::LossReason::ZeroLife => "lost:life",
                    mtg_engine::LossReason::DrewFromEmptyLibrary => "lost:decked",
                    _ => "lost:other",
                },
                _ => continue,
            };
            *counts.entry(key).or_default() += 1;
        }
        turns += game.turn().number;
        let r = match game.result() {
            Some(GameResult::Win { .. }) => "win",
            Some(GameResult::Draw { .. }) => "draw",
            Some(GameResult::Aborted { .. }) => "aborted",
            None => "none",
        };
        *results.entry(r.to_owned()).or_default() += 1;
    }
    eprintln!(
        "results {results:?}, avg turns {:.1}",
        f64::from(turns) / games as f64
    );
    eprintln!("{counts:#?}");
    for key in [
        "spell_cast",
        "attack",
        "block",
        "combat_damage",
        "spell_damage",
        "destroyed",
        "life_gained",
        "land_played",
        "mulligan",
        "lost:life",
        "decision:assign_damage",
        "decision:legend_rule",
        "decision:bottom_card",
        "decision:target",
        "decision:pay_mana",
        "decision:mana_step",
    ] {
        assert!(
            counts.get(key).copied().unwrap_or(0) > 0,
            "{key} never happened: {counts:?}"
        );
    }
}

#[test]
fn two_player_games_finish() {
    for seed in 100..120 {
        play(2, seed, true);
    }
}

#[test]
fn replaying_choices_reproduces_the_game() {
    for seed in 0..10 {
        let (game, choices) = play(4, seed, false);
        let (pool, setup) = setup(4, seed);
        let mut again = Game::new(pool, &setup).unwrap_or_else(|e| panic!("{e}"));
        for c in choices {
            again.apply_option(c).unwrap_or_else(|e| panic!("{e}"));
        }
        assert_eq!(again.state_hash(), game.state_hash(), "seed {seed}");
        assert_eq!(again.result(), game.result());
    }
}

#[test]
fn every_offered_option_applies() {
    // Invariant 3: each option of every decision applies to a clone without error,
    // and the clone stays consistent.
    let (pool, setup) = setup(4, 7);
    let sizes: Vec<usize> = setup.decks.iter().map(Vec::len).collect();
    let mut game = Game::new(pool, &setup).unwrap_or_else(|e| panic!("{e}"));
    let mut rng = GameRng::new(99);
    let mut decisions = 0;
    while !game.is_over() && decisions < 3_000 {
        let n = game
            .pending()
            .map_or(0, mtg_engine::PendingDecision::option_count);
        for i in 0..n {
            let mut g = game.clone();
            g.apply_option(u32::try_from(i).unwrap_or(0))
                .unwrap_or_else(|e| panic!("option {i}: {e}"));
            g.check_invariants(&sizes)
                .unwrap_or_else(|e| panic!("option {i} of decision {decisions}: {e}"));
        }
        let c = pick(&mut rng, &game);
        game.apply_option(c).unwrap_or_else(|e| panic!("{e}"));
        decisions += 1;
    }
}

#[test]
fn clone_then_apply_matches_apply() {
    // Invariant 4: clone().apply(c) ≡ apply(c).
    let (game, choices) = play(3, 5, false);
    let (pool, setup) = setup(3, 5);
    let mut a = Game::new(pool, &setup).unwrap_or_else(|e| panic!("{e}"));
    for c in choices {
        let mut b = a.clone();
        b.apply_option(c).unwrap_or_else(|e| panic!("{e}"));
        a.apply_option(c).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(a.state_hash(), b.state_hash());
    }
    assert_eq!(a.state_hash(), game.state_hash());
}

#[test]
fn invalid_choices_are_rejected_without_change() {
    let (pool, setup) = setup(4, 1);
    let mut game = Game::new(pool, &setup).unwrap_or_else(|e| panic!("{e}"));
    let before = game.state_hash();
    let id = game
        .pending()
        .map(|d| d.id)
        .unwrap_or_else(|| panic!("decision"));
    assert!(game.apply_option(99).is_err());
    assert!(
        game.apply(mtg_engine::Choice {
            decision: mtg_engine::DecisionId(id.0 + 1),
            option: 0
        })
        .is_err()
    );
    assert_eq!(game.state_hash(), before);
    let _ = PlayerId(0);
}
