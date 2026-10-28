//! The puzzle suite (`docs/14 Players & Policies/Measuring Play Quality.md`,
//! method 2; M9 plan S9): positions with a known answer, to measure whether
//! a policy finds it.
//!
//! A puzzle (`kiln-puzzle/1`) is a scenario board (`kiln-scenario/1`, whose
//! script sets the position up) plus who solves it and how:
//! - `win_now`: the solver wins before turn `start + within_turns` ends;
//! - `stop_the_win`: no opponent wins in that time and the solver survives.
//!
//! The solver's decisions go to the policy under test; the opponents play
//! `greedy` or `passive`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kiln_engine::{CardPool, GameResult};
use kiln_policy::{GreedyPolicy, PassivePolicy, Policy};
use kiln_types::PlayerId;
use serde::{Deserialize, Serialize};

use crate::scenario::{self, Scenario};

pub const PUZZLE_FORMAT: &str = "kiln-puzzle/1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PuzzleKind {
    WinNow,
    StopTheWin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Opponents {
    #[default]
    Greedy,
    Passive,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Puzzle {
    pub puzzle: String,
    pub name: String,
    pub kind: PuzzleKind,
    /// `P1`, `P2`, …
    pub solver: String,
    /// Turns after the one the puzzle starts in (0 = this turn only).
    #[serde(default)]
    pub within_turns: u32,
    #[serde(default)]
    pub opponents: Opponents,
    pub board: Scenario,
}

/// One puzzle's result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Solved {
    pub file: String,
    pub name: String,
    pub kind: PuzzleKind,
    pub solved: bool,
    /// How the attempt ended, in words.
    pub outcome: String,
    /// Decisions the solver made.
    pub decisions: u32,
}

pub fn load(path: &Path) -> Result<Puzzle, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let p: Puzzle = yaml_serde::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if p.puzzle != PUZZLE_FORMAT {
        return Err(format!(
            "{}: expected `puzzle: {PUZZLE_FORMAT}`",
            path.display()
        ));
    }
    Ok(p)
}

/// The puzzle files in `dir`, sorted.
pub fn files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "yaml"))
        .collect();
    v.sort();
    Ok(v)
}

fn parse_player(s: &str) -> Result<PlayerId, String> {
    s.strip_prefix('P')
        .and_then(|n| n.parse::<u8>().ok())
        .filter(|n| *n >= 1)
        .map(|n| PlayerId(n - 1))
        .ok_or_else(|| format!("solver must be P1, P2, …, not {s:?}"))
}

/// Sets the puzzle up and plays it out with `solver` for the solving seat.
pub fn attempt(
    p: &Puzzle,
    file: &str,
    pool: &Arc<CardPool>,
    solver: &mut dyn Policy,
) -> Result<Solved, String> {
    let me = parse_player(&p.solver)?;
    let mut game = scenario::run(&p.board, pool).map_err(|e| format!("set-up: {e}"))?;
    let n = game.player_count();
    let mut others: Vec<Box<dyn Policy>> = (0..n)
        .map(|_| match p.opponents {
            Opponents::Greedy => Box::new(GreedyPolicy::default()) as Box<dyn Policy>,
            Opponents::Passive => Box::new(PassivePolicy),
        })
        .collect();
    for (i, o) in others.iter_mut().enumerate() {
        o.begin_game(PlayerId(u8::try_from(i).unwrap_or(0)), 1 + i as u64);
    }
    solver.begin_game(me, 1);
    let last_turn = game.turn().number + p.within_turns;
    let mut decisions = 0u32;
    let mut steps = 0u32;
    while let Some(d) = game.pending().cloned() {
        if game.turn().number > last_turn || steps > 50_000 {
            break;
        }
        steps += 1;
        let decider = game.decider_for(d.player);
        let count = d.option_count();
        let choice = if count == 1 {
            0
        } else if decider == me {
            decisions += 1;
            solver.decide(&game.view(decider), &d)
        } else {
            others[decider.0 as usize].decide(&game.view(decider), &d)
        };
        game.apply_option(choice.min(u32::try_from(count - 1).unwrap_or(0)))
            .map_err(|e| format!("engine rejected an option: {e}"))?;
    }
    let (solved, outcome) = match (game.result(), p.kind) {
        (Some(GameResult::Win { player, turn }), _) => (
            *player == me,
            format!("P{} won on turn {turn}", player.0 + 1),
        ),
        (Some(GameResult::Draw { .. }), PuzzleKind::StopTheWin) => (true, "a draw".into()),
        (Some(r), _) => (false, format!("{r:?}")),
        (None, PuzzleKind::WinNow) => (false, format!("no win by turn {last_turn}")),
        (None, PuzzleKind::StopTheWin) => (
            game.player_state(me).in_game(),
            format!("nobody won by turn {last_turn}"),
        ),
    };
    Ok(Solved {
        file: file.to_owned(),
        name: p.name.clone(),
        kind: p.kind,
        solved,
        outcome,
        decisions,
    })
}

/// Solve rates by kind.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub policy: String,
    pub win_now: (u32, u32),
    pub stop_the_win: (u32, u32),
    pub results: Vec<Solved>,
    pub errors: Vec<String>,
}

/// Attempts every puzzle in `dir` with fresh policies from `make`.
pub fn run_suite(
    dir: &Path,
    pool: &Arc<CardPool>,
    make: &dyn Fn() -> Box<dyn Policy>,
) -> Result<Summary, String> {
    let mut s = Summary {
        policy: make().name(),
        ..Summary::default()
    };
    for f in files(dir)? {
        let label = f
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let result = load(&f).and_then(|p| attempt(&p, &label, pool, make().as_mut()));
        match result {
            Ok(r) => {
                let slot = match r.kind {
                    PuzzleKind::WinNow => &mut s.win_now,
                    PuzzleKind::StopTheWin => &mut s.stop_the_win,
                };
                slot.0 += u32::from(r.solved);
                slot.1 += 1;
                s.results.push(r);
            }
            Err(e) => s.errors.push(format!("{label}: {e}")),
        }
    }
    Ok(s)
}
