//! Batch runner skeleton (`docs/13 Simulation/Batch Runner & Parallelism.md`).
//!
//! Games run in parallel across threads, but every game's seed and every
//! seat's policy seed derive from `(experiment seed, game index, seat)`, and
//! results are collected in index order, so the output doesn't depend on
//! scheduling (ADR-0005, NFR-D1).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::time::Instant;

use mtg_core::PlayerId;
use mtg_engine::rng::{game_seed, splitmix64};
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
}

impl BatchStats {
    pub fn clean(&self) -> bool {
        self.violation_count == 0 && self.replay_failure_count == 0 && self.panic_count == 0
    }

    fn add(&mut self, o: &GameOutcome) {
        self.games += 1;
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
    while let Some(d) = game.pending() {
        let n = d.option_count();
        let option = if n == 1 {
            0
        } else {
            outcome.policy_calls += 1;
            let seat = d.player.0 as usize;
            let d = d.clone();
            let choice = policies[seat].decide(&game.view(d.player), &d);
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
        if checks.invariants
            && let Err(e) = game.check_invariants(&sizes)
        {
            outcome.violation = Some(format!("after decision {}: {e}", outcome.decisions));
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
    let replay = record.then(|| recorder.finish(&game, setup));
    (outcome, replay)
}

/// Runs a batch. `decks(game_index)` supplies each game's decks (one per seat)
/// and `policies(game_index)` its policies.
pub fn run_batch<D, P>(pool: &Arc<CardPool>, cfg: &BatchConfig, decks: D, policies: P) -> BatchStats
where
    D: Fn(u64) -> Vec<Vec<DefId>> + Sync,
    P: Fn(u64) -> Vec<Box<dyn Policy>> + Sync,
{
    let start = Instant::now();
    let work = || {
        (0..cfg.games)
            .into_par_iter()
            .map(|index| {
                let seed = game_seed(cfg.seed, index);
                let setup = GameSetup {
                    rules: cfg.rules.clone(),
                    decks: decks(index),
                    seed,
                    first_player: None,
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
