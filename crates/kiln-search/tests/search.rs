//! The search player on small synthetic positions (M9 9.2).

use std::sync::Arc;

use kiln_engine::{CardPool, Game};
use kiln_policy::{GreedyPolicy, Policy};
use kiln_search::{SearchConfig, SearchPolicy, evaluate};
use kiln_testkit::scenario;
use kiln_types::PlayerId;

fn board(text: &str) -> Game {
    let pool = Arc::new(CardPool::m3_test_set());
    let s = scenario::parse(text, "search").unwrap_or_else(|e| panic!("{e}"));
    scenario::run(&s, &pool).unwrap_or_else(|e| panic!("{e}"))
}

fn pick(policy: &mut dyn Policy, game: &Game) -> String {
    let d = game.pending().expect("a decision").clone();
    policy.begin_game(d.player, 7);
    let i = policy.decide(&game.view(d.player), &d);
    game.option_label(i as usize).expect("a label")
}

const BOLT: &str = "scenario: kiln-scenario/1
name: bolt for lethal
players:
  - battlefield: [Test Mountain]
    hand: [Test Bolt]
  - battlefield: [Test Bear]
    life: 3
turn: { number: 3, active: P1, step: precombat_main }
script:
  - choose: Cast Test Bolt
";

#[test]
fn greedy_bolts_the_bear_but_search_finds_lethal() {
    let game = board(BOLT);
    assert_eq!(
        pick(&mut GreedyPolicy::default(), &game),
        "Target Test Bear (P2)"
    );
    let mut search = SearchPolicy::greedy(SearchConfig {
        iterations: 16,
        ..SearchConfig::default()
    });
    assert_eq!(pick(&mut search, &game), "Target P2");
    assert!(search.stats()["search:decisions"] >= 1);
}

#[test]
fn search_is_deterministic() {
    let game = board(BOLT);
    let config = SearchConfig {
        iterations: 24,
        ..SearchConfig::default()
    };
    let a = pick(&mut SearchPolicy::greedy(config.clone()), &game);
    let b = pick(&mut SearchPolicy::greedy(config), &game);
    assert_eq!(a, b);
}

#[test]
#[ignore = "diagnostic"]
fn trace_puzzle() {
    // KILN_TRACE=tests/puzzles/<file>.yaml: print every decision of a search attempt.
    let (Some(db), Ok(file)) = (kiln_testkit::real::locate_db(), std::env::var("KILN_TRACE"))
    else {
        return;
    };
    let root = kiln_testkit::real::repo_root();
    let defs =
        kiln_cards::defs::load_dir(&root.join("cards/defs")).unwrap_or_else(|e| panic!("{e}"));
    let pool = Arc::new(kiln_testkit::real::real_pool(&db, &defs).0);
    let p = kiln_testkit::puzzle::load(&root.join(file)).unwrap_or_else(|e| panic!("{e}"));
    let me = PlayerId(p.solver[1..].parse::<u8>().unwrap_or(1) - 1);
    let mut game = scenario::run(&p.board, &pool).unwrap_or_else(|e| panic!("{e}"));
    let last = game.turn().number + p.within_turns;
    let mut search = SearchPolicy::greedy(SearchConfig::default());
    search.begin_game(me, 1);
    let mut greedy = GreedyPolicy::default();
    while let Some(d) = game.pending().cloned() {
        if game.turn().number > last {
            break;
        }
        let i = if d.option_count() == 1 {
            0
        } else if d.player == me {
            let i = search.decide(&game.view(me), &d);
            for o in search.last_search() {
                println!("    {:>4} visits {:>4}  {}", o.visits, o.value, o.option);
            }
            i
        } else {
            greedy.decide(&game.view(d.player), &d)
        };
        println!(
            "{:?} -> {}",
            d.player,
            game.option_label(i as usize).unwrap_or_default()
        );
        game.apply_option(i).unwrap_or_else(|e| panic!("{e}"));
    }
    println!(
        "{:?} life {:?}",
        game.result(),
        game.player_state(PlayerId(1)).life
    );
}

#[test]
fn evaluation_prefers_the_healthier_player() {
    let game = board(BOLT);
    let p1 = evaluate(&game, PlayerId(0));
    let p2 = evaluate(&game, PlayerId(1));
    assert!(p1 > p2, "{p1} vs {p2}");
    assert!((0..=1000).contains(&p1) && (0..=1000).contains(&p2));
}

#[test]
fn puzzles_meet_the_v1_bar() {
    // Measuring Play Quality: ≥ 80% of win-now and ≥ 60% of stop-the-win
    // puzzles at the default budget. Needs the real card DB.
    let Some(db) = kiln_testkit::real::locate_db() else {
        return;
    };
    let root = kiln_testkit::real::repo_root();
    let defs =
        kiln_cards::defs::load_dir(&root.join("cards/defs")).unwrap_or_else(|e| panic!("{e}"));
    let pool = Arc::new(kiln_testkit::real::real_pool(&db, &defs).0);
    let make = || Box::new(SearchPolicy::greedy(SearchConfig::default())) as Box<dyn Policy>;
    let s = kiln_testkit::puzzle::run_suite(&root.join("tests/puzzles"), &pool, &make)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(s.errors.is_empty(), "{:?}", s.errors);
    let (w, wn) = s.win_now;
    let (st, stn) = s.stop_the_win;
    assert!(w * 10 >= wn * 8, "win now {w}/{wn}");
    assert!(st * 10 >= stn * 6, "stop the win {st}/{stn}");
}
