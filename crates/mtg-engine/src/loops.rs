//! Loops and shortcuts (CR 732, 104.4b; `docs/03 Architecture/Subsystems/Loops & Shortcuts.md`,
//! M4 plan D9–D10).
//!
//! At each priority decision (once a step has seen enough of them), the state
//! is hashed *modulo monotonic quantities*: life, poison, mana pools and
//! counter counts. A repeat of that hash with the same player holding
//! priority is a loop; the change in the monotonic quantities between the
//! two points is one iteration's delta.
//!
//! - If only the priority holder made choices in between, they may propose a
//!   shortcut (CR 732.2a): repeat it N times. Each other player, in turn
//!   order, accepts or shortens it (CR 732.2b). The shortcut applies N times
//!   the delta in one step (CR 732.2c).
//! - If nobody made a choice (everyone only passed), the loop is mandatory
//!   (CR 732.4). It runs to the iteration where a player loses, or, if it
//!   never ends, the game is a draw (CR 104.4b). Players may still shorten it
//!   to act (CR 732.5).
//! - Loops where several players make choices (CR 732.3) aren't shortcut.
//!
//! Identity: objects keep their ids across iterations only if they don't
//! change zones, so loops that move cards (Underworld Breach) aren't detected
//! yet (M4 note, known limitations).

use std::hash::{Hash, Hasher};

use mtg_cards::ir::CounterKind;
use mtg_core::{ObjectId, PlayerId};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::decision::DecisionKind;
use crate::engine::{Game, GameResult, Task};
use crate::events::Event;
use crate::state::{StableHasher, Step};

/// Priority decisions in one step before loop detection starts hashing; a
/// step this busy is rare outside loops, so normal games pay nothing.
const LOOP_MIN_DECISIONS: u32 = 16;
/// Recent loop points remembered per step.
const HISTORY: usize = 256;
/// The largest shortcut (D10).
pub const MAX_ITERATIONS: u64 = 1_000_000_000;
/// Shortcut sizes offered to the proposer (any other count composes from
/// these and playing iterations out by hand).
const MENU: [u64; 5] = [10, 100, 1_000, 1_000_000, MAX_ITERATIONS];

/// The quantities a loop may change without breaking it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct Monotonic {
    pools: SmallVec<[[u32; 6]; 8]>,
    life: SmallVec<[i64; 8]>,
    poison: SmallVec<[u32; 8]>,
    counters: Vec<(ObjectId, CounterKind, u32)>,
}

/// One iteration's change in the monotonic quantities.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct LoopDelta {
    pools: SmallVec<[[i64; 6]; 8]>,
    life: SmallVec<[i64; 8]>,
    poison: SmallVec<[i64; 8]>,
    counters: Vec<(ObjectId, CounterKind, i64)>,
}

impl LoopDelta {
    fn is_zero(&self) -> bool {
        self.pools.iter().all(|p| p.iter().all(|d| *d == 0))
            && self.life.iter().all(|d| *d == 0)
            && self.poison.iter().all(|d| *d == 0)
            && self.counters.iter().all(|(_, _, d)| *d == 0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct LoopPoint {
    hash: u64,
    holder: PlayerId,
    choices: [u32; 8],
    at: Monotonic,
}

/// Loop detection state (not part of the state hash: it is a function of
/// the decisions taken, like the event log).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct LoopTracker {
    turn: u32,
    step: Option<Step>,
    decisions: u32,
    history: Vec<LoopPoint>,
    /// Non-pass decisions each player has made (CR 732.4 "mandatory").
    pub(crate) choices: [u32; 8],
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum ShortcutKind {
    /// A player proposes repeating their loop (CR 732.2a).
    Proposed(PlayerId),
    /// Nobody made a choice: the loop continues unless someone acts (CR 732.4).
    Mandatory,
}

/// The CR 732.2 procedure for one detected loop.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct ShortcutTask {
    kind: ShortcutKind,
    delta: Box<LoopDelta>,
    /// Iterations proposed so far (`None`: not chosen yet, or endless).
    n: Option<u64>,
    /// The most iterations the loop's resources allow.
    max: u64,
    /// Whether the loop ends the game by itself (someone loses).
    terminal: bool,
    responders: SmallVec<[PlayerId; 8]>,
    next: u8,
}

impl Game {
    /// The loop identity of the current state: everything except the
    /// monotonic quantities, the decision bookkeeping and the event log.
    fn loop_hash(&self) -> u64 {
        // Objects that changed zones and came back to the same arena slot
        // count as the same (Q26: Worldgorger Dragon, Underworld Breach).
        mtg_core::hash_ids_by_slot(|| self.loop_hash_by_slot())
    }

    fn loop_hash_by_slot(&self) -> u64 {
        let mut h = StableHasher::default();
        for p in &self.players {
            (
                p.id,
                &p.library,
                &p.hand,
                &p.graveyard,
                p.lands_played,
                p.status,
                p.drew_from_empty,
            )
                .hash(&mut h);
        }
        // Not timestamps: a loop re-creates objects with later ones (CR 613.7).
        for (id, o) in self.objects.iter() {
            (
                id,
                o.def,
                o.owner,
                o.controller,
                o.zone,
                o.tapped,
                o.damage,
                o.deathtouch_damage,
                o.control_since,
            )
                .hash(&mut h);
        }
        // Entries of objects that left are kept until cleanup (LKI); not those.
        for a in &self.attachments {
            if self.obj(a.0).is_some() {
                a.hash(&mut h);
            }
        }
        self.entering_hosts.hash(&mut h);
        self.battlefield.hash(&mut h);
        self.stack.hash(&mut h);
        self.exile.hash(&mut h);
        self.turn.hash(&mut h);
        self.combat.hash(&mut h);
        self.rng.draws().hash(&mut h);
        self.tasks.hash(&mut h);
        // Effects without their timestamps: a loop may recreate one each time.
        for e in &self.effects {
            (&e.affected, &e.effect, e.duration).hash(&mut h);
        }
        self.activated_this_turn.hash(&mut h);
        self.pending_triggers.hash(&mut h);
        self.delayed.hash(&mut h);
        self.face_down.hash(&mut h);
        self.linked.hash(&mut h);
        self.copies.hash(&mut h);
        (self.drew_in_draw_step, self.combat_damaged).hash(&mut h);
        // Which kinds of counters are where, not how many.
        for (id, kind, n) in &self.counters {
            if *n > 0 {
                (id, kind).hash(&mut h);
            }
        }
        // "First ... each turn" only depends on which spells were cast.
        let mut cast: Vec<(PlayerId, crate::card::DefId)> = self
            .cast_this_turn
            .iter()
            .map(|r| (r.player, r.def))
            .collect();
        cast.sort_unstable();
        cast.dedup();
        cast.hash(&mut h);
        h.finish()
    }

    fn monotonic(&self) -> Monotonic {
        Monotonic {
            pools: self.players.iter().map(|p| p.pool.0).collect(),
            life: self.players.iter().map(|p| p.life).collect(),
            poison: self.players.iter().map(|p| p.poison).collect(),
            counters: self.counters.clone(),
        }
    }

    /// Records a priority decision for `holder`; returns a shortcut procedure
    /// when this point repeats an earlier one.
    pub(crate) fn loop_check(&mut self, holder: PlayerId) -> Option<ShortcutTask> {
        let (turn, step) = (self.turn.number, self.turn.step);
        if self.loops.turn != turn || self.loops.step != Some(step) {
            self.loops.turn = turn;
            self.loops.step = Some(step);
            self.loops.decisions = 0;
            self.loops.history.clear();
        }
        self.loops.decisions += 1;
        if self.loops.decisions < LOOP_MIN_DECISIONS {
            return None;
        }
        let hash = self.loop_hash();
        let at = self.monotonic();
        let choices = self.loops.choices;
        let earlier = self
            .loops
            .history
            .iter()
            .rev()
            .find(|e| e.hash == hash && e.holder == holder)
            .cloned();
        if self.loops.history.len() >= HISTORY {
            self.loops.history.remove(0);
        }
        self.loops.history.push(LoopPoint {
            hash,
            holder,
            choices,
            at: at.clone(),
        });
        let e = earlier?;
        let chose: SmallVec<[PlayerId; 8]> = self
            .players
            .iter()
            .map(|p| p.id)
            .filter(|p| choices[p.0 as usize] != e.choices[p.0 as usize])
            .collect();
        let delta = delta(&e.at, &at);
        let (max, terminal) = self.iteration_limit(&delta, &at);
        let kind = match chose.as_slice() {
            [] => ShortcutKind::Mandatory,
            [p] if *p == holder && !delta.is_zero() && max > 0 => ShortcutKind::Proposed(holder),
            _ => return None,
        };
        let responders = match kind {
            // CR 732.2b: the other players, in turn order after the proposer.
            ShortcutKind::Proposed(p) => self
                .turn_order_from(p)
                .into_iter()
                .filter(|q| *q != p)
                .collect(),
            // APNAP order (CR 101.4).
            ShortcutKind::Mandatory => self.turn_order_from(self.turn.active).into_iter().collect(),
        };
        let n = match kind {
            ShortcutKind::Proposed(_) => None,
            ShortcutKind::Mandatory => (terminal || max < MAX_ITERATIONS).then_some(max),
        };
        Some(ShortcutTask {
            kind,
            delta: Box::new(delta),
            n,
            max,
            terminal,
            responders,
            next: 0,
        })
    }

    /// How many iterations the resources allow, and whether the loop ends
    /// the game by then (a player's life reaches 0 or poison 10).
    fn iteration_limit(&self, d: &LoopDelta, now: &Monotonic) -> (u64, bool) {
        let mut max = MAX_ITERATIONS;
        let mut terminal = false;
        let cap = |have: i64, per: i64| -> u64 { u64::try_from(have.max(0) / per).unwrap_or(0) };
        for (p, pool) in d.pools.iter().enumerate() {
            for (t, &per) in pool.iter().enumerate() {
                if per < 0 {
                    max = max.min(cap(i64::from(now.pools[p][t]), -per));
                }
            }
        }
        for &(id, kind, per) in &d.counters {
            let have = now
                .counters
                .iter()
                .find(|(o, k, _)| *o == id && *k == kind)
                .map_or(0, |(_, _, n)| i64::from(*n));
            // Shrinking toughness changes the board mid-loop: not shortcut.
            if (kind == CounterKind::MinusOne && per > 0)
                || (kind == CounterKind::PlusOne && per < 0)
            {
                return (0, false);
            }
            if per < 0 {
                max = max.min(cap(have, -per));
            }
        }
        for (p, &per) in d.life.iter().enumerate() {
            if per < 0 && self.players[p].in_game() {
                // CR 704.5a: the iteration that takes them to 0 or less.
                let life = now.life[p].max(0);
                let n = u64::try_from((life + (-per) - 1) / (-per))
                    .unwrap_or(0)
                    .max(1);
                if n <= max {
                    max = n;
                    terminal = true;
                }
            }
        }
        for (p, &per) in d.poison.iter().enumerate() {
            if per > 0 && self.players[p].in_game() {
                let left = (10 - i64::from(now.poison[p])).max(0);
                let n = u64::try_from((left + per - 1) / per).unwrap_or(0).max(1);
                if n <= max {
                    max = n;
                    terminal = true;
                }
            }
        }
        (max, terminal)
    }

    pub(crate) fn shortcut_step(&mut self, mut t: ShortcutTask) {
        if let ShortcutKind::Proposed(p) = t.kind
            && t.n.is_none()
        {
            let counts = menu(t.max);
            self.tasks.push(Task::Shortcut(t));
            self.decide(p, DecisionKind::ProposeShortcut { counts }, "732.2a");
            return;
        }
        while let Some(&who) = t.responders.get(usize::from(t.next)) {
            if self.in_game(who) {
                let proposed = t.n;
                let mut stops = vec![0];
                stops.extend(
                    MENU.iter()
                        .copied()
                        .filter(|k| proposed.is_none_or(|n| *k < n)),
                );
                self.tasks.push(Task::Shortcut(t));
                self.decide(
                    who,
                    DecisionKind::RespondShortcut { proposed, stops },
                    "732.2b",
                );
                return;
            }
            t.next += 1;
        }
        self.finish_shortcut(&t);
    }

    pub(crate) fn shortcut_answer(
        &mut self,
        mut t: ShortcutTask,
        kind: DecisionKind,
        option: usize,
    ) {
        match kind {
            DecisionKind::ProposeShortcut { counts } => {
                if option == 0 {
                    // Declined: play on; the loop is offered again after another cycle.
                    self.loops.history.clear();
                    return;
                }
                t.n = Some(counts[option - 1]);
            }
            DecisionKind::RespondShortcut { stops, .. } => {
                if option > 0 {
                    // CR 732.2b: shortened to where this player acts.
                    let k = stops[option - 1];
                    t.n = Some(t.n.map_or(k, |n| n.min(k)));
                    t.terminal = false;
                }
                t.next += 1;
            }
            _ => unreachable!("shortcut task answered with {kind:?}"),
        }
        self.shortcut_step(t);
    }

    /// CR 732.2c: the game advances past the agreed iterations; an endless
    /// mandatory loop nobody breaks is a draw (CR 104.4b).
    fn finish_shortcut(&mut self, t: &ShortcutTask) {
        self.loops.history.clear();
        let Some(n) = t.n else {
            self.log(Event::LoopDraw);
            self.result = Some(GameResult::Draw {
                turn: self.turn.number,
            });
            return;
        };
        self.log(Event::ShortcutTaken {
            proposer: match t.kind {
                ShortcutKind::Proposed(p) => Some(p),
                ShortcutKind::Mandatory => None,
            },
            iterations: n,
        });
        if n == 0 {
            return;
        }
        let n = i64::try_from(n).unwrap_or(i64::MAX);
        let d = &t.delta;
        for (i, p) in self.players.iter_mut().enumerate() {
            for (k, &per) in d.pools[i].iter().enumerate() {
                let v = i64::from(p.pool.0[k]).saturating_add(per.saturating_mul(n));
                p.pool.0[k] = u32::try_from(v.max(0)).unwrap_or(u32::MAX);
            }
            p.life = p.life.saturating_add(d.life[i].saturating_mul(n));
            let poison = i64::from(p.poison).saturating_add(d.poison[i].saturating_mul(n));
            p.poison = u32::try_from(poison.max(0)).unwrap_or(u32::MAX);
        }
        for &(id, kind, per) in &d.counters {
            let amount = u32::try_from(per.unsigned_abs().saturating_mul(n.unsigned_abs()))
                .unwrap_or(u32::MAX);
            if per > 0 {
                self.add_counters(id, kind, amount);
            } else if per < 0 {
                self.remove_counters(id, kind, amount);
            }
        }
        // Life, poison or toughness may have changed: check SBAs before priority.
        self.tasks.push(Task::Sba(crate::sba::SbaTask::default()));
    }
}

/// The shortcut sizes `max` iterations allow (plus `max` itself when the
/// loop's resources or the game's end limit it).
fn menu(max: u64) -> Vec<u64> {
    let mut v: Vec<u64> = MENU.iter().copied().filter(|k| *k < max).collect();
    v.push(max.min(MAX_ITERATIONS));
    v.dedup();
    v
}

fn delta(a: &Monotonic, b: &Monotonic) -> LoopDelta {
    let pools = a
        .pools
        .iter()
        .zip(&b.pools)
        .map(|(x, y)| std::array::from_fn(|t| i64::from(y[t]) - i64::from(x[t])))
        .collect();
    let life = a.life.iter().zip(&b.life).map(|(x, y)| y - x).collect();
    let poison = a
        .poison
        .iter()
        .zip(&b.poison)
        .map(|(x, y)| i64::from(*y) - i64::from(*x))
        .collect();
    let mut keys: Vec<(ObjectId, CounterKind)> = a
        .counters
        .iter()
        .chain(&b.counters)
        .map(|(o, k, _)| (*o, *k))
        .collect();
    keys.sort_unstable();
    keys.dedup();
    let count = |m: &Monotonic, id: ObjectId, kind: CounterKind| {
        m.counters
            .iter()
            .find(|(o, k, _)| *o == id && *k == kind)
            .map_or(0, |(_, _, n)| i64::from(*n))
    };
    let counters = keys
        .into_iter()
        .map(|(id, kind)| (id, kind, count(b, id, kind) - count(a, id, kind)))
        .filter(|(_, _, d)| *d != 0)
        .collect();
    LoopDelta {
        pools,
        life,
        poison,
        counters,
    }
}
