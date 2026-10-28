//! Determinization (M9 plan S1): a copy consistent with one player's view.

use std::sync::Arc;

use kiln_engine::decks::test_deck;
use kiln_engine::rng::GameRng;
use kiln_engine::{CardPool, DecisionKind, DefId, Game, GameRules, GameSetup};
use kiln_types::PlayerId;

fn game(seed: u64, steps: usize) -> Game {
    let pool = Arc::new(CardPool::test_set());
    let setup = GameSetup {
        rules: GameRules {
            log_events: false,
            ..GameRules::default()
        },
        decks: (0..4).map(|i| test_deck(&pool, i, 60)).collect(),
        seed,
        first_player: None,
        commanders: Vec::new(),
    };
    let mut g = Game::new(pool, &setup).unwrap_or_else(|e| panic!("{e}"));
    let mut rng = GameRng::new(seed);
    for _ in 0..steps {
        let Some(d) = g.pending() else { break };
        let n = d.option_count();
        let pass = matches!(d.kind, DecisionKind::Priority { .. }) && rng.below(2) == 0;
        let c = if pass { 0 } else { rng.index(n) };
        g.apply_option(u32::try_from(c).unwrap_or(0))
            .unwrap_or_else(|e| panic!("{e}"));
    }
    g
}

/// The definitions in `p`'s hand and library, sorted.
fn hidden_defs(g: &Game, p: PlayerId) -> (Vec<DefId>, Vec<DefId>) {
    let defs = |ids: &[kiln_types::ObjectId]| {
        let mut v: Vec<DefId> = ids
            .iter()
            .filter_map(|id| g.object(*id).map(|o| o.def))
            .collect();
        v.sort_unstable();
        v
    };
    let pl = g.player_state(p);
    (defs(&pl.hand), defs(&pl.library))
}

#[test]
fn the_viewer_sees_the_same_game() {
    for seed in 0..20 {
        let g = game(seed, 300);
        let me = g.pending().map_or(PlayerId(0), |d| d.player);
        let d = g.view(me).determinize(seed * 7 + 1);
        assert_eq!(g.view(me).snapshot(), d.view(me).snapshot(), "seed {seed}");
        assert_eq!(g.pending().map(|p| &p.kind), d.pending().map(|p| &p.kind));
        d.audit_view(me).unwrap_or_else(|e| panic!("{e}"));
    }
}

#[test]
fn hidden_cards_are_redealt_but_kept() {
    let mut changed = 0;
    for seed in 0..20 {
        let g = game(seed, 300);
        let me = PlayerId(0);
        let d = g.view(me).determinize(seed);
        let (my_hand, my_library) = hidden_defs(&g, me);
        assert_eq!(hidden_defs(&d, me), (my_hand, my_library), "own cards kept");
        for p in 1..4u8 {
            let p = PlayerId(p);
            let (h, l) = hidden_defs(&g, p);
            let (dh, dl) = hidden_defs(&d, p);
            assert_eq!(h.len(), dh.len(), "hand size kept");
            assert_eq!(l.len(), dl.len(), "library size kept");
            let mut before: Vec<DefId> = h.iter().chain(&l).copied().collect();
            let mut after: Vec<DefId> = dh.iter().chain(&dl).copied().collect();
            before.sort_unstable();
            after.sort_unstable();
            assert_eq!(before, after, "the same cards, redealt");
            changed += usize::from(h != dh);
        }
    }
    assert!(
        changed > 30,
        "opponents' hands should usually change ({changed}/60)"
    );
}

#[test]
fn the_same_seed_gives_the_same_sample() {
    let g = game(5, 300);
    let a = g.view(PlayerId(1)).determinize(42);
    let b = g.view(PlayerId(1)).determinize(42);
    let c = g.view(PlayerId(1)).determinize(43);
    assert_eq!(a.state_hash(), b.state_hash());
    assert_ne!(a.state_hash(), c.state_hash());
}

#[test]
fn a_determinized_game_plays_on() {
    for seed in 0..10 {
        let g = game(seed, 200);
        let me = g.pending().map_or(PlayerId(0), |d| d.player);
        let mut d = g.view(me).determinize(seed);
        let mut rng = GameRng::new(seed);
        let mut steps = 0;
        while let Some(p) = d.pending() {
            let n = p.option_count();
            d.apply_option(u32::try_from(rng.index(n)).unwrap_or(0))
                .unwrap_or_else(|e| panic!("{e}"));
            steps += 1;
            if steps > 20_000 {
                break;
            }
        }
        assert!(steps > 0);
    }
}
