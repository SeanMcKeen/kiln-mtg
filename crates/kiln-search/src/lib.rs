//! `kiln-search`: the search player (`docs/14 Players & Policies/D - Search AI.md`,
//! M9 plan S1–S8).
//!
//! Single-observer information-set MCTS over the searching player's own
//! decisions. Each iteration samples a game consistent with the player's view
//! ([`PlayerView::determinize`]), walks the tree of its decision sequences
//! (children keyed by option label, so they mean the same thing in every
//! sample), with the guide policy's pick as the prior; every other decision
//! is played by greedy (for every seat), and the end of a truncated playout
//! is scored with [`evaluate_after`].
//!
//! All arithmetic is integer (values in thousandths, `isqrt`), so a search
//! with an iteration budget picks the same option on every platform
//! (ADR-0005).

use std::sync::Arc;
use std::time::Instant;

use kiln_engine::decision::{CardPurpose, OptionalKind};
use kiln_engine::rng::splitmix64;
use kiln_engine::{Action, DecisionKind, Game, GameResult, PendingDecision, PlayerView, ZoneId};
use kiln_policy::{GreedyPolicy, Policy};
use kiln_types::{CardType, ObjectId, PlayerId};

/// The value of a win, in thousandths.
pub const WIN: i64 = 1000;

/// How hard to search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchConfig {
    /// Iterations (determinized playouts) per searched decision.
    pub iterations: u32,
    /// Playouts stop after this many real choices (decisions with more than
    /// one option), then [`evaluate`] scores the position.
    pub rollout_depth: u32,
    /// A wall-clock cap per decision in milliseconds. Results then depend on
    /// the machine (ADR-0005); reports warn.
    pub max_ms: Option<u64>,
    /// The PUCT exploration constant, in thousandths.
    pub exploration: i64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            iterations: 32,
            rollout_depth: 100,
            max_ms: None,
            exploration: 700,
        }
    }
}

/// Makes a fresh pilot for the searching seat (its guide policy).
pub type PolicyFactory = Arc<dyn Fn() -> Box<dyn Policy> + Send + Sync>;

/// The search player.
pub struct SearchPolicy {
    config: SearchConfig,
    /// Answers the decisions search skips, in the real game.
    guide: Box<dyn Policy>,
    /// Makes the pilot that plays the searching seat inside simulations and
    /// gives the priors (a separate instance, so simulated calls never reach
    /// the guide's counters).
    make_pilot: PolicyFactory,
    pilot: Option<Box<dyn Policy>>,
    name: String,
    seat: PlayerId,
    seed: u64,
    decisions: u64,
    searched: u64,
    iterations: u64,
    /// The real game's turn, the abilities search activated in it, and how
    /// many decisions it searched in it.
    turn: u32,
    activated: Vec<(ObjectId, u8)>,
    searched_this_turn: u32,
    last: Vec<RootOption>,
}

/// One option of the last searched decision: how often search tried it and
/// its mean value in thousandths of a win.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootOption {
    pub option: String,
    pub visits: u32,
    pub value: i64,
}

/// Searches per turn before the guide takes over: a combo turn can offer
/// hundreds of decisions, and search's cost should stay bounded.
const SEARCHES_PER_TURN: u32 = 40;

/// How many options a non-priority decision considers at once.
const WIDE: usize = 8;

/// Halfway through the budget, search stops if every option it tried is
/// within this much of the others (thousandths of a win)…
const INDIFFERENT: i64 = 15;
/// …or the best is ahead of the rest by this much.
const CLEAR: i64 = 200;

impl std::fmt::Debug for SearchPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchPolicy")
            .field("config", &self.config)
            .field("guide", &self.name)
            .finish_non_exhaustive()
    }
}

impl SearchPolicy {
    /// Search guided by greedy.
    pub fn greedy(config: SearchConfig) -> Self {
        Self::guided(
            config,
            "greedy".into(),
            Arc::new(|| Box::new(GreedyPolicy::default()) as Box<dyn Policy>),
        )
    }

    /// Search guided by the policies `make` builds (a playbook with a
    /// fallback): one answers what search skips, others pilot simulations.
    pub fn guided(config: SearchConfig, guide_name: String, make: PolicyFactory) -> Self {
        Self {
            config,
            guide: make(),
            make_pilot: make,
            pilot: None,
            name: guide_name,
            seat: PlayerId(0),
            seed: 0,
            decisions: 0,
            searched: 0,
            iterations: 0,
            turn: 0,
            activated: Vec::new(),
            searched_this_turn: 0,
            last: Vec::new(),
        }
    }

    pub fn config(&self) -> &SearchConfig {
        &self.config
    }

    /// The options of the last searched decision, as search saw them.
    pub fn last_search(&self) -> &[RootOption] {
        &self.last
    }
}

impl Policy for SearchPolicy {
    fn name(&self) -> String {
        format!(
            "search({} iterations, depth {}, guide {})",
            self.config.iterations, self.config.rollout_depth, self.name
        )
    }

    fn begin_game(&mut self, seat: PlayerId, seed: u64) {
        self.seat = seat;
        self.seed = seed;
        self.decisions = 0;
        self.searched = 0;
        self.iterations = 0;
        self.turn = 0;
        self.activated.clear();
        self.searched_this_turn = 0;
        self.guide.begin_game(seat, seed);
        let mut pilot = (self.make_pilot)();
        pilot.begin_game(seat, splitmix64(seed ^ 0x5EA2C));
        self.pilot = Some(pilot);
    }

    fn decide(&mut self, view: &PlayerView<'_>, d: &PendingDecision) -> u32 {
        self.decisions += 1;
        let turn = view.turn().number;
        if turn != self.turn {
            self.turn = turn;
            self.activated.clear();
            self.searched_this_turn = 0;
        }
        if !worth_searching(view, d) || self.searched_this_turn >= SEARCHES_PER_TURN {
            return self.guide.decide(view, d);
        }
        let seed = splitmix64(self.seed ^ self.decisions.wrapping_mul(0x9E37_79B9));
        let pilot = self.pilot.get_or_insert_with(|| (self.make_pilot)());
        let mut search = Search {
            config: &self.config,
            me: view.me(),
            pilot: pilot.as_mut(),
            nodes: vec![Node::default()],
            root_activated: &self.activated,
        };
        let result = search.run(view, d, seed);
        self.last = search.nodes[0]
            .children
            .iter()
            .map(|c| RootOption {
                option: c.key.clone(),
                visits: c.visits,
                value: c.value / i64::from(c.visits.max(1)),
            })
            .collect();
        let Some((choice, iterations)) = result else {
            return self.guide.decide(view, d);
        };
        self.searched += 1;
        self.searched_this_turn += 1;
        self.iterations += u64::from(iterations);
        if let Some(a) = activation(d, choice) {
            self.activated.push(a);
        }
        choice
    }

    fn stats(&self) -> std::collections::BTreeMap<String, u64> {
        let mut m = self.guide.stats();
        m.insert("search:decisions".into(), self.searched);
        m.insert("search:iterations".into(), self.iterations);
        m.insert(
            "search:delegated".into(),
            self.decisions.saturating_sub(self.searched),
        );
        m
    }

    fn take_errors(&mut self) -> Vec<String> {
        self.guide.take_errors()
    }
}

/// Decisions worth a search (M9 plan S6); the rest go to the guide.
fn worth_searching(view: &PlayerView<'_>, d: &PendingDecision) -> bool {
    match &d.kind {
        DecisionKind::Priority { actions } => {
            actions
                .iter()
                .filter(|a| !matches!(a, Action::ActivateMana { .. }))
                .count()
                >= 2
        }
        DecisionKind::ChooseTarget { .. }
        | DecisionKind::DeclareAttacker { .. }
        | DecisionKind::DeclareBlocker { .. }
        | DecisionKind::Mulligan { .. }
        | DecisionKind::Discard { .. }
        | DecisionKind::ChooseMode { .. }
        | DecisionKind::ChooseName { .. }
        | DecisionKind::ChoosePlayer { .. } => d.option_count() >= 2,
        DecisionKind::ChooseCard { purpose, .. } => {
            matches!(
                purpose,
                CardPurpose::Search
                    | CardPurpose::Take
                    | CardPurpose::Discard
                    | CardPurpose::Sacrifice
                    | CardPurpose::Exile
            ) && d.option_count() >= 2
        }
        DecisionKind::Optional { kind, .. } => matches!(
            kind,
            OptionalKind::May
                | OptionalKind::IfDoes
                | OptionalKind::PayUnless
                | OptionalKind::Kicker
        ),
        _ => {
            let _ = view;
            false
        }
    }
}

/// The ability option `i` activates, if it activates one.
fn activation(d: &PendingDecision, i: u32) -> Option<(ObjectId, u8)> {
    match &d.kind {
        DecisionKind::Priority { actions } => match actions.get(i as usize) {
            Some(Action::Activate { source, ability }) => Some((*source, *ability)),
            _ => None,
        },
        _ => None,
    }
}

/// The options search considers, as (key, option index), in prior order: the
/// pilot's pick, then passing, then the rest. Bare mana abilities at priority
/// are left out (payment activates them), and so are abilities already
/// activated this turn: like greedy, search activates each at most once a
/// turn, so loops that change nothing (a monolith untapping itself) end.
/// Keys are labels, numbered when two options share one.
fn candidates(
    view: &PlayerView<'_>,
    d: &PendingDecision,
    pick: u32,
    activated: &[(ObjectId, u8)],
) -> Vec<(String, u32)> {
    let n = d.option_count();
    let mut seen: Vec<String> = Vec::with_capacity(n);
    let mut keyed: Vec<(String, u32)> = Vec::with_capacity(n);
    for i in 0..n {
        let label = view
            .option_label(i)
            .unwrap_or_else(|| format!("option {i}"));
        let dup = seen.iter().filter(|l| **l == label).count();
        seen.push(label.clone());
        let key = if dup == 0 {
            label
        } else {
            format!("{label} #{}", dup + 1)
        };
        let i = u32::try_from(i).unwrap_or(u32::MAX);
        let mana = matches!(
            &d.kind,
            DecisionKind::Priority { actions }
                if matches!(actions.get(i as usize), Some(Action::ActivateMana { .. }))
        );
        if (mana && i != pick) || activation(d, i).is_some_and(|a| activated.contains(&a)) {
            continue;
        }
        keyed.push((key, i));
    }
    let rank = |i: u32| -> u8 {
        if i == pick {
            0
        } else if i == 0 && matches!(d.kind, DecisionKind::Priority { .. }) {
            1
        } else {
            2
        }
    };
    keyed.sort_by_key(|(_, i)| (rank(*i), *i));
    keyed
}

#[derive(Debug, Default)]
struct Node {
    visits: u32,
    children: Vec<Child>,
}

#[derive(Debug)]
struct Child {
    key: String,
    /// Position in prior order when first seen (0 = the pilot's pick).
    rank: u32,
    visits: u32,
    /// Iterations in which this option was offered and considered.
    avail: u32,
    value: i64,
    node: Option<usize>,
}

struct Search<'a> {
    config: &'a SearchConfig,
    me: PlayerId,
    pilot: &'a mut dyn Policy,
    nodes: Vec<Node>,
    /// Abilities activated this turn in the real game.
    root_activated: &'a [(ObjectId, u8)],
}

impl Search<'_> {
    /// Searches the root decision; returns the chosen option and the
    /// iterations run, or `None` when fewer than two options remain.
    fn run(&mut self, view: &PlayerView<'_>, d: &PendingDecision, seed: u64) -> Option<(u32, u32)> {
        let pick = self.pilot.decide(view, d);
        let root = candidates(view, d, pick, self.root_activated);
        if root.len() < 2 {
            return None;
        }
        let started = Instant::now();
        let mut done = 0;
        for it in 0..self.config.iterations {
            if self
                .config
                .max_ms
                .is_some_and(|ms| started.elapsed().as_millis() >= u128::from(ms))
            {
                break;
            }
            if it == self.config.iterations / 2 && self.settled() {
                break;
            }
            let iseed = splitmix64(seed ^ u64::from(it).wrapping_mul(0xA24B_AED4_963E_E407));
            let mut game = view.determinize(iseed);
            self.iterate(&mut game, iseed);
            done += 1;
        }
        // The best mean value among options tried enough (at least twice and
        // a quarter as often as the most tried): with small budgets, visit
        // counts follow the prior more than the values do.
        let children = &self.nodes[0].children;
        let most = children.iter().map(|c| c.visits).max().unwrap_or(0);
        let enough = (most / 4).max(2).min(most);
        let best = children
            .iter()
            .filter(|c| c.visits >= enough)
            .max_by_key(|c| {
                (
                    c.value / i64::from(c.visits.max(1)),
                    c.visits,
                    std::cmp::Reverse(c.rank),
                )
            })?;
        let index = root.iter().find(|(k, _)| *k == best.key).map(|(_, i)| *i)?;
        Some((index, done))
    }

    /// Halfway through the budget: whether more iterations are unlikely to
    /// change the choice, because the options tried at least twice are all
    /// within [`INDIFFERENT`] of each other, or the best leads by [`CLEAR`].
    fn settled(&self) -> bool {
        let mut means: Vec<i64> = self.nodes[0]
            .children
            .iter()
            .filter(|c| c.visits >= 2)
            .map(|c| c.value / i64::from(c.visits))
            .collect();
        if means.len() < 2 || means.len() < self.nodes[0].children.len() {
            return false;
        }
        means.sort_unstable_by(|a, b| b.cmp(a));
        means[0] - means[means.len() - 1] < INDIFFERENT || means[0] - means[1] >= CLEAR
    }

    /// One iteration: descend the tree, expand one node, play out, back up.
    fn iterate(&mut self, game: &mut Game, seed: u64) {
        let me = self.me;
        let n = game.player_count();
        let mut others: Vec<GreedyPolicy> = (0..n).map(|_| GreedyPolicy::default()).collect();
        for (p, o) in others.iter_mut().enumerate() {
            o.begin_game(seat(p), splitmix64(seed ^ p as u64));
        }
        self.pilot.begin_game(me, splitmix64(seed ^ 0x9170));
        let mut node = Some(0usize);
        let mut path: Vec<(usize, usize)> = Vec::new();
        let mut activated = self.root_activated.to_vec();
        let mut turn = game.turn().number;
        let start_turn = turn;
        let mut choices = 0u32;
        let mut steps = 0u32;
        let cap = self.config.rollout_depth.saturating_mul(20).max(1000);
        while let Some(d) = game.pending().cloned() {
            if !game.player_state(me).in_game()
                || choices >= self.config.rollout_depth
                || steps >= cap
            {
                break;
            }
            steps += 1;
            let count = d.option_count();
            let decider = game.decider_for(d.player);
            let option = if count == 1 {
                0
            } else {
                choices += 1;
                let view = game.view(decider);
                if view.turn().number != turn {
                    turn = view.turn().number;
                    activated.clear();
                }
                match node {
                    Some(at) if decider == me && worth_searching(&view, &d) => {
                        let pick = self.pilot.decide(&view, &d);
                        let options = candidates(&view, &d, pick, &activated);
                        let all = !matches!(d.kind, DecisionKind::Priority { .. });
                        match self.select(at, &options, all) {
                            Some((child, option)) => {
                                path.push((at, child));
                                if let Some(a) = activation(&d, option) {
                                    activated.push(a);
                                }
                                node = match self.nodes[at].children[child].node {
                                    Some(next) => Some(next),
                                    None => {
                                        // Expand one node per iteration, then play out.
                                        self.nodes.push(Node::default());
                                        let next = self.nodes.len() - 1;
                                        self.nodes[at].children[child].node = Some(next);
                                        None
                                    }
                                };
                                option
                            }
                            None => pick,
                        }
                    }
                    // Playouts are greedy for every seat, the searcher's too: a
                    // playbook call per playout decision made guided search
                    // five times slower. The guide still gives the priors.
                    _ => others[decider.0 as usize].decide(&view, &d),
                }
            };
            let option = option.min(u32::try_from(count - 1).unwrap_or(0));
            if game.apply_option(option).is_err() {
                break;
            }
        }
        let value = evaluate_after(game, me, game.turn().number.saturating_sub(start_turn));
        for (at, child) in path {
            self.nodes[at].visits += 1;
            let c = &mut self.nodes[at].children[child];
            c.visits += 1;
            c.value += value;
        }
    }

    /// PUCT over the options available in this sample, widened progressively:
    /// a node with `v` visits considers its first `1 + isqrt(v)` options
    /// (at least two) in prior order. Returns (child, option index).
    fn select(&mut self, at: usize, options: &[(String, u32)], all: bool) -> Option<(usize, u32)> {
        if options.is_empty() {
            return None;
        }
        let visits = self.nodes[at].visits;
        // Targets, attacks, blocks and modes are few: consider them all (up
        // to eight). Priority can offer dozens of actions: widen with visits.
        let width = if all {
            WIDE
        } else {
            (1 + visits.isqrt()).max(3) as usize
        };
        let considered = &options[..options.len().min(width)];
        let node = &mut self.nodes[at];
        let mut avail: Vec<(usize, u32)> = Vec::with_capacity(considered.len());
        for (rank, (key, option)) in considered.iter().enumerate() {
            let child = match node.children.iter().position(|c| c.key == *key) {
                Some(c) => c,
                None => {
                    node.children.push(Child {
                        key: key.clone(),
                        rank: u32::try_from(rank).unwrap_or(u32::MAX),
                        visits: 0,
                        avail: 0,
                        value: 0,
                        node: None,
                    });
                    node.children.len() - 1
                }
            };
            node.children[child].avail += 1;
            avail.push((child, *option));
        }
        // Unvisited options first, in prior order.
        if let Some(&(child, option)) = avail.iter().find(|(c, _)| node.children[*c].visits == 0) {
            return Some((child, option));
        }
        // The pilot's pick carries 40% of the prior; the rest share 60%. Values
        // differ by tens of thousandths, so exploration stays modest.
        let k = i64::try_from(avail.len()).unwrap_or(1).max(2);
        let c = self.config.exploration;
        avail
            .iter()
            .max_by_key(|(child, option)| {
                let ch = &node.children[*child];
                let prior = if ch.rank == 0 { 400 } else { 600 / (k - 1) };
                let q = ch.value / i64::from(ch.visits.max(1));
                let sqrt_avail =
                    i64::try_from((u64::from(ch.avail) * 1_000_000).isqrt()).unwrap_or(0);
                let u = c * prior / 1000 * sqrt_avail / 1000 / (1 + i64::from(ch.visits));
                (q + u, std::cmp::Reverse(*option))
            })
            .copied()
    }
}

fn seat(p: usize) -> PlayerId {
    PlayerId(u8::try_from(p).unwrap_or(u8::MAX))
}

/// A win is worth 25 less for each turn it takes, down to [`SLOW_WIN`]:
/// sooner is better, since opponents get turns to stop it.
const WIN_DISCOUNT: i64 = 25;
/// The least a win is worth; any unfinished position scores below it.
pub const SLOW_WIN: i64 = 850;
/// A loss is worth 5 for each turn survived, up to 40 (below every
/// unfinished position): a later loss leaves opponents time to stumble.
const LOSS_DELAY: i64 = 5;
const LATE_LOSS: i64 = 40;
/// Unfinished positions score in [`HEURISTIC_FLOOR`, `HEURISTIC_FLOOR + HEURISTIC_RANGE`].
const HEURISTIC_FLOOR: i64 = 50;
const HEURISTIC_RANGE: i64 = 750;

/// The value of a position for `me` reached `elapsed` turns after the
/// search began, in thousandths of a win (M9 plan S5). Finished: a win is
/// 1000 less 25 per turn taken (at least [`SLOW_WIN`]), a loss 5 per turn
/// survived (at most 40), a draw an equal share. Otherwise `me`'s share of the living players' scores, where a
/// score adds life (up to twice the starting 40), cards in hand, lands, other
/// permanents and creature power, less poison; scaled into 50–800 so that
/// every win beats every unfinished position.
pub fn evaluate_after(game: &Game, me: PlayerId, elapsed: u32) -> i64 {
    let late_loss = (LOSS_DELAY * i64::from(elapsed)).min(LATE_LOSS);
    if let Some(GameResult::Win { player, .. }) = game.result() {
        return if *player == me {
            (WIN - WIN_DISCOUNT * i64::from(elapsed)).max(SLOW_WIN)
        } else {
            late_loss
        };
    }
    if !game.player_state(me).in_game() {
        return late_loss;
    }
    evaluate(game, me)
}

/// [`evaluate_after`] with no time taken: a win is 1000.
pub fn evaluate(game: &Game, me: PlayerId) -> i64 {
    match game.result() {
        Some(GameResult::Win { player, .. }) => return if *player == me { WIN } else { 0 },
        Some(GameResult::Draw { .. }) => {
            let alive = (0..game.player_count())
                .filter(|p| game.player_state(seat(*p)).in_game())
                .count()
                .max(1);
            return WIN / i64::try_from(alive).unwrap_or(1);
        }
        _ => {}
    }
    if !game.player_state(me).in_game() {
        return 0;
    }
    let n = game.player_count();
    let mut scores = vec![0i64; n];
    for (p, s) in scores.iter_mut().enumerate() {
        let pl = game.player_state(seat(p));
        if !pl.in_game() {
            continue;
        }
        *s = 10 * pl.life.clamp(0, 80) + 60 * i64::try_from(pl.hand.len()).unwrap_or(0)
            - 100 * i64::from(pl.poison);
    }
    let view = game.view(me);
    for id in game.objects_in(ZoneId::Battlefield) {
        let Some(o) = view.object(id) else { continue };
        let p = o.controller.0 as usize;
        if p >= n || !game.player_state(o.controller).in_game() {
            continue;
        }
        let def = view.definition(o.def);
        scores[p] += if def.is(CardType::Land) {
            80
        } else if def.is(CardType::Creature) {
            40 + 20 * i64::from(o.power.unwrap_or(0).max(0))
        } else {
            50
        };
    }
    let total: i64 = scores.iter().map(|s| (*s).max(1)).sum();
    HEURISTIC_FLOOR + HEURISTIC_RANGE * scores[me.0 as usize].max(1) / total.max(1)
}
