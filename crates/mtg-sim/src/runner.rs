//! Batch runner skeleton (`docs/13 Simulation/Batch Runner & Parallelism.md`).
//!
//! Games run in parallel across threads, but every game's seed and every
//! seat's policy seed derive from `(experiment seed, game index, seat)`, and
//! results are collected in index order, so the output doesn't depend on
//! scheduling (ADR-0005, NFR-D1).

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::time::Instant;

use mtg_core::{CardType, Colors, PlayerId};
use mtg_engine::rng::{GameRng, game_seed, splitmix64};
use mtg_engine::{CardPool, DefId, Game, GameResult, GameRules, GameSetup, Recorder};
use mtg_policy::Policy;
use rayon::prelude::*;
use serde::Serialize;

/// What to check while games run.
/// With [`Checks::views`], every seat's view is audited after every this-many
/// decisions and at game end.
pub const VIEW_AUDIT_EVERY: u64 = 16;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Checks {
    /// Check engine invariants after every decision.
    pub invariants: bool,
    /// Audit every seat's view for hidden-information leaks (sampled: see [`VIEW_AUDIT_EVERY`]).
    pub views: bool,
    /// Re-run every k-th game from its replay log and compare final hashes.
    pub replay_every: Option<u64>,
    /// Chance per mille, before each decision, that the deciding player
    /// concedes (CR 104.3a), to exercise players leaving mid-game.
    #[serde(default)]
    pub concede_per_mille: u64,
    /// Harness guard (not a rule): stop a game after this many decisions and
    /// report it, so a loop nothing breaks can't run forever.
    #[serde(default)]
    pub decision_limit: Option<u64>,
}

/// One batch of games.
#[derive(Debug, Clone, Serialize)]
pub struct BatchConfig {
    pub games: u64,
    pub seed: u64,
    pub rules: GameRules,
    pub checks: Checks,
    /// Worker threads; `None` = all cores.
    pub threads: Option<usize>,
    /// Give each seat a commander that fits its deck ([`pick_commander`]).
    #[serde(default)]
    pub commanders: bool,
}

/// How each player left and how the game ended (see [`GameOutcome::endings`]).
pub fn endings(game: &Game) -> Vec<String> {
    use mtg_engine::PlayerStatus;
    let mut out: Vec<String> = (0..game.player_count())
        .filter_map(|i| {
            match game
                .player_state(PlayerId(u8::try_from(i).unwrap_or(0)))
                .status
            {
                PlayerStatus::Lost(r) => Some(format!("loss:{r:?}")),
                _ => None,
            }
        })
        .collect();
    match game.result() {
        Some(GameResult::Win { player, .. }) => {
            // An effect win (CR 104.2b) ends the game with opponents still in it;
            // otherwise the winner is the last player standing (CR 104.2a).
            let others_in = (0..game.player_count())
                .map(|i| PlayerId(u8::try_from(i).unwrap_or(0)))
                .any(|p| p != *player && game.player_state(p).status == PlayerStatus::Playing);
            out.push(
                if others_in {
                    "win:effect"
                } else {
                    "win:last_standing"
                }
                .to_owned(),
            );
        }
        Some(GameResult::Draw { .. }) => out.push("draw".to_owned()),
        _ => {}
    }
    out
}

/// A commander for a generated deck: a legendary creature card from the
/// pool whose color identity fits the deck's (CR 903.3, 903.5c), chosen with
/// `seed`; `None` if none fits.
pub fn pick_commander(pool: &CardPool, deck: &[DefId], seed: u64) -> Option<DefId> {
    let colors = deck.iter().fold(Colors::COLORLESS, |c, d| {
        c.union(pool.get(*d).color_identity)
    });
    let fits: Vec<DefId> = pool
        .iter()
        .filter(|(_, d)| {
            d.is_legendary()
                && d.is(CardType::Creature)
                && !d.token
                && d.mana_cost.is_some()
                && d.color_identity.is_subset_of(colors)
        })
        .map(|(id, _)| id)
        .collect();
    (!fits.is_empty()).then(|| fits[GameRng::new(seed).index(fits.len())])
}

/// The outcome of one game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GameOutcome {
    pub index: u64,
    pub seed: u64,
    pub result: Option<GameResult>,
    pub turns: u32,
    pub decisions: u64,
    /// Decisions answered by a policy (not the single-option fast path, R6).
    pub policy_calls: u64,
    pub violation: Option<String>,
    pub replay_checked: bool,
    pub replay_error: Option<String>,
    pub panic: Option<String>,
    /// How players left and how the game ended: `loss:<reason>` per losing
    /// player, then `win:effect` (CR 104.2b), `win:last_standing` (CR 104.2a)
    /// or `draw` (CR 104.4a).
    pub endings: Vec<String>,
}

/// Aggregate statistics.
#[derive(Debug, Clone, Default, Serialize)]
pub struct BatchStats {
    pub games: u64,
    pub wins_by_seat: Vec<u64>,
    pub draws: u64,
    pub aborted: u64,
    pub total_turns: u64,
    pub total_decisions: u64,
    pub total_policy_calls: u64,
    pub replays_checked: u64,
    /// Up to 20 failures of each kind, `(game index, seed, message)`.
    pub violations: Vec<(u64, u64, String)>,
    pub replay_failures: Vec<(u64, u64, String)>,
    pub panics: Vec<(u64, u64, String)>,
    pub violation_count: u64,
    pub replay_failure_count: u64,
    pub panic_count: u64,
    pub elapsed_ms: u64,
    /// Counts of [`GameOutcome::endings`] entries (M6 plan C15: win types).
    pub endings: BTreeMap<String, u64>,
}

impl BatchStats {
    pub fn clean(&self) -> bool {
        self.violation_count == 0 && self.replay_failure_count == 0 && self.panic_count == 0
    }

    fn add(&mut self, o: &GameOutcome) {
        self.games += 1;
        for e in &o.endings {
            *self.endings.entry(e.clone()).or_default() += 1;
        }
        self.total_turns += u64::from(o.turns);
        self.total_decisions += o.decisions;
        self.total_policy_calls += o.policy_calls;
        match o.result {
            Some(GameResult::Win { player, .. }) => {
                let i = player.0 as usize;
                if self.wins_by_seat.len() <= i {
                    self.wins_by_seat.resize(i + 1, 0);
                }
                self.wins_by_seat[i] += 1;
            }
            Some(GameResult::Draw { .. }) => self.draws += 1,
            Some(GameResult::Aborted { .. }) | None => self.aborted += 1,
        }
        if o.replay_checked {
            self.replays_checked += 1;
        }
        let push = |list: &mut Vec<(u64, u64, String)>, count: &mut u64, msg: &Option<String>| {
            if let Some(m) = msg {
                *count += 1;
                if list.len() < 20 {
                    list.push((o.index, o.seed, m.clone()));
                }
            }
        };
        push(
            &mut self.violations,
            &mut self.violation_count,
            &o.violation,
        );
        push(
            &mut self.replay_failures,
            &mut self.replay_failure_count,
            &o.replay_error,
        );
        push(&mut self.panics, &mut self.panic_count, &o.panic);
    }
}

/// Seed for seat `seat`'s policy in a game, independent of the game RNG.
pub fn policy_seed(game_seed: u64, seat: usize) -> u64 {
    splitmix64(game_seed ^ splitmix64(0x00B0_11C1 + seat as u64))
}

/// Plays one game to completion.
pub fn run_game(
    pool: &Arc<CardPool>,
    setup: GameSetup,
    policies: &mut [Box<dyn Policy>],
    checks: &Checks,
    record: bool,
) -> (GameOutcome, Option<mtg_engine::Replay>) {
    let sizes: Vec<usize> = setup.decks.iter().map(Vec::len).collect();
    let mut outcome = GameOutcome {
        index: 0,
        seed: setup.seed,
        result: None,
        turns: 0,
        decisions: 0,
        policy_calls: 0,
        violation: None,
        replay_checked: false,
        replay_error: None,
        panic: None,
        endings: Vec::new(),
    };
    let mut game = match Game::new(Arc::clone(pool), &setup) {
        Ok(g) => g,
        Err(e) => {
            outcome.violation = Some(format!("setup: {e}"));
            return (outcome, None);
        }
    };
    for (seat, p) in policies.iter_mut().enumerate() {
        p.begin_game(
            PlayerId(u8::try_from(seat).unwrap_or(0)),
            policy_seed(setup.seed, seat),
        );
    }
    let mut recorder = Recorder::new();
    let mut concede_rng = GameRng::new(splitmix64(setup.seed ^ 0x00C0_4CED));
    while let Some(d) = game.pending() {
        if checks.concede_per_mille > 0 && concede_rng.below(1000) < checks.concede_per_mille {
            let p = d.player;
            if record {
                recorder.concede(&mut game, p);
            } else {
                game.concede(p);
            }
            if checks.invariants
                && let Err(e) = game.check_invariants(&sizes)
            {
                outcome.violation = Some(format!("after {p} conceded: {e}"));
                break;
            }
            continue;
        }
        let n = d.option_count();
        let option = if n == 1 {
            0
        } else {
            outcome.policy_calls += 1;
            // CR 723.5: a controlled player's decisions are their controller's.
            let decider = game.decider_for(d.player);
            let seat = decider.0 as usize;
            let d = d.clone();
            let choice = policies[seat].decide(&game.view(decider), &d);
            if choice as usize >= n {
                outcome.violation = Some(format!(
                    "policy {} chose option {choice} of {n}",
                    policies[seat].name()
                ));
                break;
            }
            choice
        };
        let applied = if record {
            recorder.apply(&mut game, option)
        } else {
            game.apply_option(option)
        };
        if let Err(e) = applied {
            outcome.violation = Some(format!("engine rejected an offered option: {e}"));
            break;
        }
        outcome.decisions += 1;
        if checks
            .decision_limit
            .is_some_and(|cap| outcome.decisions >= cap)
        {
            outcome.violation = Some(format!(
                "harness decision limit: {} decisions, turn {}, step {:?}{}",
                outcome.decisions,
                game.turn().number,
                game.turn().step,
                last_events(&game)
            ));
            break;
        }
        if checks.invariants
            && let Err(e) = game.check_invariants(&sizes)
        {
            outcome.violation = Some(format!(
                "after decision {}: {e}{}",
                outcome.decisions,
                last_events(&game)
            ));
            break;
        }
        // View audits serialize every seat's snapshot, so they are sampled: every
        // VIEW_AUDIT_EVERY-th decision and the final state.
        if checks.views && (outcome.decisions.is_multiple_of(VIEW_AUDIT_EVERY) || game.is_over()) {
            for seat in 0..game.player_count() {
                if let Err(e) = game.audit_view(PlayerId(u8::try_from(seat).unwrap_or(0))) {
                    outcome.violation = Some(format!(
                        "after decision {}: invariant 5: {e}",
                        outcome.decisions
                    ));
                    break;
                }
            }
            if outcome.violation.is_some() {
                break;
            }
        }
    }
    outcome.result = game.result().copied();
    outcome.turns = game.turn().number;
    outcome.endings = endings(&game);
    let replay = record.then(|| recorder.finish(&game, setup));
    (outcome, replay)
}

/// One seat's deck and commanders (CR 903.3). With no commanders and
/// [`BatchConfig::commanders`], one is picked to fit the deck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatDeck {
    pub deck: Vec<DefId>,
    pub commanders: Vec<DefId>,
}

/// With event logging on, the last events (they help find a violation's cause).
fn last_events(game: &Game) -> String {
    let tail: Vec<String> = game
        .events()
        .iter()
        .rev()
        .take(12)
        .rev()
        .map(|ev| format!("{ev:?}"))
        .collect();
    if tail.is_empty() {
        String::new()
    } else {
        format!("\n      last events: {}", tail.join("\n        "))
    }
}

/// Runs a batch. `decks(game_index)` supplies each game's decks (one per seat)
/// and `policies(game_index)` its policies.
pub fn run_batch<D, P>(pool: &Arc<CardPool>, cfg: &BatchConfig, decks: D, policies: P) -> BatchStats
where
    D: Fn(u64) -> Vec<Vec<DefId>> + Sync,
    P: Fn(u64) -> Vec<Box<dyn Policy>> + Sync,
{
    run_batch_seats(
        pool,
        cfg,
        |index| {
            decks(index)
                .into_iter()
                .map(|deck| SeatDeck {
                    deck,
                    commanders: Vec::new(),
                })
                .collect()
        },
        policies,
    )
}

/// [`run_batch`] with each seat's commanders given (decklists).
pub fn run_batch_seats<D, P>(
    pool: &Arc<CardPool>,
    cfg: &BatchConfig,
    decks: D,
    policies: P,
) -> BatchStats
where
    D: Fn(u64) -> Vec<SeatDeck> + Sync,
    P: Fn(u64) -> Vec<Box<dyn Policy>> + Sync,
{
    let start = Instant::now();
    let work = || {
        (0..cfg.games)
            .into_par_iter()
            .map(|index| {
                let seed = game_seed(cfg.seed, index);
                let seats = decks(index);
                let commanders = if cfg.commanders || seats.iter().any(|s| !s.commanders.is_empty())
                {
                    seats
                        .iter()
                        .enumerate()
                        .map(|(seat, s)| {
                            if !s.commanders.is_empty() {
                                s.commanders.clone()
                            } else if cfg.commanders {
                                let k = splitmix64(seed ^ (0xC0_33A4D3 + seat as u64));
                                pick_commander(pool, &s.deck, k).into_iter().collect()
                            } else {
                                Vec::new()
                            }
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                let decks: Vec<Vec<DefId>> = seats.into_iter().map(|s| s.deck).collect();
                let setup = GameSetup {
                    rules: cfg.rules.clone(),
                    decks,
                    seed,
                    first_player: None,
                    commanders,
                };
                let check_replay = cfg
                    .checks
                    .replay_every
                    .is_some_and(|k| k > 0 && index % k == 0);
                let run = catch_unwind(AssertUnwindSafe(|| {
                    let mut pols = policies(index);
                    let (mut o, replay) =
                        run_game(pool, setup.clone(), &mut pols, &cfg.checks, check_replay);
                    if let Some(r) = replay {
                        o.replay_checked = true;
                        if let Err(e) = r.run(Arc::clone(pool)) {
                            o.replay_error = Some(e.to_string());
                        }
                    }
                    o
                }));
                let mut o = run.unwrap_or_else(|e| {
                    let msg = e
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                        .unwrap_or_else(|| "panic".into());
                    GameOutcome {
                        index,
                        seed,
                        result: None,
                        turns: 0,
                        decisions: 0,
                        policy_calls: 0,
                        violation: None,
                        replay_checked: false,
                        replay_error: None,
                        panic: Some(msg),
                        endings: Vec::new(),
                    }
                });
                o.index = index;
                o.seed = seed;
                o
            })
            .collect::<Vec<GameOutcome>>()
    };
    let outcomes = match cfg.threads {
        Some(n) => rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build()
            .map_or_else(|_| work(), |p| p.install(work)),
        None => work(),
    };
    let mut stats = BatchStats::default();
    for o in &outcomes {
        stats.add(o);
    }
    stats.elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    stats
}
