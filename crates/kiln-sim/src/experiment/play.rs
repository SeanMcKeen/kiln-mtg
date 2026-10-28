//! Playing an experiment's games (M7 plan S4–S7, S13): configurations,
//! seating, policies, limits, and one [`GameRecord`] per game.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use kiln_data::CardDb;
use kiln_engine::rng::{GameRng, game_seed, splitmix64};
use kiln_engine::{
    CardPool, DefId, Event, Game, GameResult, GameRules, GameSetup, LossReason, PlayerStatus,
    Recorder, Replay,
};
use kiln_playbook::{Playbook, PlaybookPolicy};
use kiln_policy::{Compose, GreedyPolicy, PassivePolicy, Policy, RandomPolicy};
use kiln_protocol::record::{BASE_CONFIG, Elimination, GameRecord, Outcome, Timing};
use kiln_protocol::spec::{Mode, PolicySpec, Seating};
use kiln_protocol::{ResolvedSpec, parse};
use kiln_search::{PolicyFactory, SearchConfig, SearchPolicy};
use kiln_types::{OracleId, PlayerId};

use super::{ExperimentError, ResolvedDeck, facts, resolve_decks};
use crate::library::Library;
use crate::runner::policy_seed;

/// The deck id of a goldfish opponent.
pub const OPPONENT: &str = "opponent";

/// A finished game, with its replay and raw events when asked for.
#[derive(Debug)]
pub struct Played {
    pub record: GameRecord,
    pub replay: Option<Replay>,
    pub events: Option<Vec<Event>>,
}

/// One deck as card ids in the experiment's pool.
#[derive(Debug, Clone)]
struct PlayDeck {
    id: String,
    cards: Vec<DefId>,
    commanders: Vec<DefId>,
    policy: PolicySpec,
}

/// A line-up: the decks that play together (the base pod, or an A/B variant
/// swapped in for its base).
#[derive(Debug, Clone)]
pub struct Config {
    pub name: String,
    /// Indices into the experiment's decks, one per seat before seating.
    slots: Vec<usize>,
}

/// A resolved experiment, ready to play.
#[derive(Debug)]
pub struct Experiment {
    pub resolved: ResolvedSpec,
    /// The directory the spec's deck paths are relative to.
    pub spec_dir: Option<std::path::PathBuf>,
    pub decks: Vec<ResolvedDeck>,
    pub warnings: Vec<String>,
    pub card_db: String,
    pool: Arc<CardPool>,
    play: Vec<PlayDeck>,
    configs: Vec<Config>,
    /// Compiled playbooks by their path in the spec.
    books: Books,
}

impl Experiment {
    /// Resolves the decks (legality, coverage gate) and maps them onto the
    /// card library's pool.
    pub fn prepare(
        resolved: ResolvedSpec,
        spec_dir: &Path,
        db: &CardDb,
        lib: &Library,
    ) -> Result<Self, ExperimentError> {
        let spec = &resolved.spec;
        let (decks, mut warnings) = resolve_decks(spec, spec_dir, db, lib)?;
        let mut books = BTreeMap::new();
        for path in parse::playbook_paths(spec) {
            let file = spec_dir.join(&path);
            let text = std::fs::read_to_string(&file).map_err(|e| {
                ExperimentError::Playbook(format!("can't read playbook {}: {e}", file.display()))
            })?;
            let book = Playbook::compile(&path, &text)
                .map_err(|e| ExperimentError::Playbook(e.to_string()))?;
            books.insert(path, Arc::new(book));
        }
        warnings.splice(0..0, resolved.warnings.iter().cloned());
        let (pool, ids) = lib.pool();
        let def = |o: &OracleId, deck: &str| -> Result<DefId, ExperimentError> {
            ids.get(o).copied().ok_or_else(|| ExperimentError::Deck {
                id: deck.to_owned(),
                message: format!("card {o} isn't in the card library's pool"),
            })
        };
        let policy_for = |d: &ResolvedDeck| -> PolicySpec {
            let p = &spec.policies;
            p.get(&d.id)
                .or_else(|| d.base.as_ref().and_then(|b| p.get(b)))
                .or_else(|| p.get("default"))
                .cloned()
                .unwrap_or_default()
        };
        let mut play = Vec::new();
        for d in &decks {
            let mut cards = Vec::new();
            for (o, qty) in &d.cards {
                let id = def(o, &d.id)?;
                cards.extend(std::iter::repeat_n(id, *qty as usize));
            }
            let commanders = d
                .commanders
                .iter()
                .map(|o| def(o, &d.id))
                .collect::<Result<_, _>>()?;
            play.push(PlayDeck {
                id: d.id.clone(),
                cards,
                commanders,
                policy: policy_for(d),
            });
        }
        let base_slots: Vec<usize> = (0..spec.decks.len()).collect();
        let mut configs = vec![Config {
            name: BASE_CONFIG.into(),
            slots: base_slots.clone(),
        }];
        match spec.mode {
            Mode::Goldfish => {
                let island = pool.by_name("Island").ok_or_else(|| {
                    ExperimentError::Run("the pool has no Island for goldfish opponents".into())
                })?;
                let size = play[0].cards.len() + play[0].commanders.len();
                let opponents = spec.goldfish.clone().unwrap_or_default().opponents;
                play.push(PlayDeck {
                    id: OPPONENT.into(),
                    cards: vec![island; size],
                    commanders: Vec::new(),
                    policy: PolicySpec::Passive,
                });
                let opp = play.len() - 1;
                configs[0].slots = std::iter::once(0)
                    .chain(std::iter::repeat_n(opp, usize::from(opponents)))
                    .collect();
            }
            Mode::Ab => {
                for (i, d) in decks.iter().enumerate().skip(spec.decks.len()) {
                    let Some(base) = d.base.as_ref() else {
                        continue;
                    };
                    let Some(b) = spec.decks.iter().position(|x| x.id == *base) else {
                        continue;
                    };
                    let mut slots = base_slots.clone();
                    slots[b] = i;
                    configs.push(Config {
                        name: d.id.clone(),
                        slots,
                    });
                }
            }
            Mode::OpeningHands => {
                // One configuration per deck: it and three passive opponents, a
                // Commander pod, so the first mulligan is free (CR 103.5c).
                let island = pool.by_name("Island").ok_or_else(|| {
                    ExperimentError::Run("the pool has no Island for the dummy opponent".into())
                })?;
                play.push(PlayDeck {
                    id: OPPONENT.into(),
                    cards: vec![island; 99],
                    commanders: Vec::new(),
                    policy: PolicySpec::Passive,
                });
                let opp = play.len() - 1;
                configs = (0..spec.decks.len())
                    .map(|i| Config {
                        name: play[i].id.clone(),
                        slots: vec![i, opp, opp, opp],
                    })
                    .collect();
            }
            Mode::Pod => {}
        }
        let kinds: std::collections::BTreeSet<&str> = play
            .iter()
            .filter(|d| d.id != OPPONENT)
            .map(|d| d.policy.kind())
            .collect();
        if kinds.len() > 1 {
            warnings.push(format!(
                "decks are played by different policy kinds ({}): results compare pilots as well as decks",
                kinds.into_iter().collect::<Vec<_>>().join(", ")
            ));
        }
        Ok(Self {
            card_db: db.id(),
            spec_dir: std::fs::canonicalize(spec_dir).ok(),
            resolved,
            decks,
            warnings,
            pool: Arc::new(pool),
            play,
            configs,
            books: Arc::new(books),
        })
    }

    pub fn configs(&self) -> &[Config] {
        &self.configs
    }

    pub fn pool(&self) -> &Arc<CardPool> {
        &self.pool
    }

    /// Games per configuration.
    pub fn games(&self) -> u64 {
        self.resolved.spec.games
    }

    /// The slot each seat holds in game `index` (M7 plan S4).
    fn seating(&self, index: u64, n: usize) -> Vec<usize> {
        let spec = &self.resolved.spec;
        if spec.mode == Mode::OpeningHands {
            return (0..n).collect();
        }
        match spec.seating {
            Seating::Fixed => (0..n).collect(),
            Seating::BalancedRotation => {
                let r = usize::try_from(index % n as u64).unwrap_or(0);
                (0..n).map(|s| (s + r) % n).collect()
            }
            Seating::AllPermutations => {
                let k = parse::rotation_count(Seating::AllPermutations, n);
                nth_permutation(n, index % k)
            }
            Seating::Random => {
                let mut rng = GameRng::new(splitmix64(game_seed(spec.seed, index) ^ 0x5EA7));
                let mut order: Vec<usize> = (0..n).collect();
                for i in (1..n).rev() {
                    order.swap(i, rng.index(i + 1));
                }
                order
            }
        }
    }

    /// [`Experiment::play`] with the event log on (for inspecting a game).
    pub fn play_logged(&self, config: &Config, index: u64) -> Option<(Played, Option<Game>)> {
        let never = AtomicBool::new(false);
        self.play_inner(config, index, &never, true, true)
    }

    /// The game setup and seat decks for game `index` of `config`.
    pub fn setup(&self, config: &Config, index: u64) -> (GameSetup, Vec<usize>) {
        let spec = &self.resolved.spec;
        let n = config.slots.len();
        let seats: Vec<usize> = self
            .seating(index, n)
            .into_iter()
            .map(|slot| config.slots[slot])
            .collect();
        let mut rules = GameRules::commander();
        if let Some(life) = spec.goldfish.as_ref().and_then(|g| g.life) {
            rules.starting_life = life;
        }
        rules.turn_limit = Some(spec.limits.max_turns);
        rules.log_events = spec.metrics.needs_events();
        let setup = GameSetup {
            rules,
            decks: seats.iter().map(|d| self.play[*d].cards.clone()).collect(),
            seed: game_seed(spec.seed, index),
            // Seat 1 plays first; seating moves the decks (CR 103.1).
            first_player: Some(PlayerId(0)),
            commanders: seats
                .iter()
                .map(|d| self.play[*d].commanders.clone())
                .collect(),
        };
        (setup, seats)
    }

    fn policies(&self, seats: &[usize]) -> Vec<Box<dyn Policy>> {
        seats
            .iter()
            .map(|d| self.build_policy(&self.play[*d].policy))
            .collect()
    }

    fn build_policy(&self, p: &PolicySpec) -> Box<dyn Policy> {
        build_policy(p, &self.books)
    }

    /// The policy recipe of each deck, for reports.
    pub fn recipes(&self) -> BTreeMap<String, String> {
        self.play
            .iter()
            .map(|d| (d.id.clone(), self.build_policy(&d.policy).name()))
            .collect()
    }

    /// Plays game `index` of `config`. `None` if cancelled.
    pub fn play_game(
        &self,
        config: &Config,
        index: u64,
        cancel: &AtomicBool,
    ) -> Option<GameRecord> {
        self.play(config, index, cancel, false).map(|p| p.record)
    }

    /// [`Experiment::play_game`], also recording a replay if asked and the raw
    /// events if `metrics.raw_events` is on. Games are deterministic, so
    /// playing an index again gives the same game (this is how replays are kept).
    pub fn play(
        &self,
        config: &Config,
        index: u64,
        cancel: &AtomicBool,
        replay: bool,
    ) -> Option<Played> {
        self.play_inner(config, index, cancel, replay, false)
            .map(|(p, _)| p)
    }

    fn play_inner(
        &self,
        config: &Config,
        index: u64,
        cancel: &AtomicBool,
        replay: bool,
        log_events: bool,
    ) -> Option<(Played, Option<Game>)> {
        let spec = &self.resolved.spec;
        let (mut setup, seats) = self.setup(config, index);
        setup.rules.log_events |= log_events;
        let opening = spec.mode == Mode::OpeningHands;
        let goldfish = spec.mode == Mode::Goldfish;
        let mut recorder = replay.then(Recorder::new);
        let mut hands: Option<Vec<Vec<DefId>>> = None;
        let n = seats.len();
        let started = Instant::now();
        let mut record = GameRecord {
            config: config.name.clone(),
            index,
            seed: setup.seed,
            seats: seats.iter().map(|d| self.play[*d].id.clone()).collect(),
            outcome: Outcome::Capped,
            winner: None,
            win_type: None,
            turns: 0,
            eliminations: Vec::new(),
            mulligans: vec![0; n],
            decisions: 0,
            capped: None,
            decision_kinds: spec.metrics.decision_stats.then(BTreeMap::new),
            timing: None,
            cards: None,
            interaction: None,
            mana: None,
            hand: None,
            policy_stats: None,
            policy_errors: None,
        };
        let mut game = match Game::new(Arc::clone(&self.pool), &setup) {
            Ok(g) => g,
            Err(e) => {
                record.capped = Some(format!("setup failed: {e}"));
                return Some((
                    Played {
                        record,
                        replay: None,
                        events: None,
                    },
                    None,
                ));
            }
        };
        let mut policies = self.policies(&seats);
        for (seat, p) in policies.iter_mut().enumerate() {
            p.begin_game(seat_id(seat), policy_seed(setup.seed, seat));
        }
        let mut policy_us = vec![0u64; n];
        while let Some(d) = game.pending() {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            // The first decision of turn 1: every opening hand is final.
            if hands.is_none() && game.turn().number >= 1 {
                hands = Some(
                    (0..n)
                        .map(|seat| {
                            game.player_state(seat_id(seat))
                                .hand
                                .iter()
                                .filter_map(|id| game.object(*id).map(|o| o.def))
                                .collect()
                        })
                        .collect(),
                );
                if opening {
                    break;
                }
            }
            if record.decisions >= spec.limits.max_decisions {
                record.capped = Some("max_decisions".into());
                break;
            }
            if spec
                .limits
                .max_wall_ms
                .is_some_and(|ms| started.elapsed().as_millis() >= u128::from(ms))
            {
                record.capped = Some("max_wall_ms".into());
                break;
            }
            if let Some(k) = &mut record.decision_kinds {
                *k.entry(kind_name(&d.kind)).or_default() += 1;
            }
            let count = d.option_count();
            let option = if count == 1 {
                0
            } else {
                // CR 723.5: a controlled player's decisions are their controller's.
                let decider = game.decider_for(d.player);
                let seat = decider.0 as usize;
                let d = d.clone();
                let t = Instant::now();
                let choice = policies[seat].decide(&game.view(decider), &d);
                policy_us[seat] += u64::try_from(t.elapsed().as_micros()).unwrap_or(u64::MAX);
                choice.min(u32::try_from(count - 1).unwrap_or(0))
            };
            let applied = match &mut recorder {
                Some(r) => r.apply(&mut game, option),
                None => game.apply_option(option),
            };
            if applied.is_err() {
                record.capped = Some("engine rejected an offered option".into());
                break;
            }
            record.decisions += 1;
            note_eliminations(&game, &mut record);
            // Goldfish: once the measured deck is out, only passive players remain.
            if goldfish
                && record
                    .seats
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| *d != OPPONENT)
                    .all(|(seat, _)| record.eliminations.iter().any(|e| e.seat == seat))
            {
                record.outcome = Outcome::Lost;
                break;
            }
        }
        record.turns = game.turn().number;
        for (seat, m) in record.mulligans.iter_mut().enumerate() {
            *m = game.player_state(seat_id(seat)).mulligans;
        }
        let hands = hands.unwrap_or_default();
        match game.result() {
            _ if opening => {
                record.outcome = Outcome::NotPlayed;
                record.capped = None;
                record.hand = hands
                    .first()
                    .map(|h| h.iter().map(|d| self.pool.get(*d).name.clone()).collect());
            }
            Some(GameResult::Win { player, .. }) => {
                record.outcome = Outcome::Win;
                record.winner = Some(player.0 as usize);
                record.win_type = Some(win_type(&game, *player, &record));
            }
            Some(GameResult::Draw { .. }) => record.outcome = Outcome::Draw,
            Some(GameResult::Aborted { .. }) => {
                record.capped.get_or_insert_with(|| "max_turns".into());
            }
            None => {}
        }
        if spec.metrics.timing {
            record.timing = Some(Timing {
                total_us: u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
                policy_us,
            });
        }
        let m = &spec.metrics;
        if !opening && (m.per_card.enabled || m.interaction || m.mana) {
            let tracked: Vec<bool> = record
                .seats
                .iter()
                .map(|d| {
                    m.per_card.enabled
                        && d != OPPONENT
                        && (m.per_card.decks.is_empty() || m.per_card.decks.contains(d))
                })
                .collect();
            let f = facts::extract(
                &game,
                &hands,
                &setup.commanders,
                facts::Want {
                    card_seats: &tracked,
                    interaction: m.interaction,
                    mana: m.mana,
                },
            );
            record.cards = f.cards;
            record.interaction = f.interaction;
            record.mana = f.mana;
        }
        let stats: Vec<BTreeMap<String, u64>> = policies.iter().map(|p| p.stats()).collect();
        if stats.iter().any(|s| !s.is_empty()) {
            record.policy_stats = Some(stats);
        }
        let errors: Vec<String> = policies
            .iter_mut()
            .enumerate()
            .flat_map(|(seat, p)| {
                p.take_errors()
                    .into_iter()
                    .map(move |e| format!("seat {}: {e}", seat + 1))
            })
            .take(5)
            .collect();
        if !errors.is_empty() {
            record.policy_errors = Some(errors);
        }
        let events = m.raw_events.then(|| game.events().to_vec());
        let replay = recorder.map(|r| r.finish(&game, setup));
        Some((
            Played {
                record,
                replay,
                events,
            },
            Some(game),
        ))
    }

    /// Resolves and prepares an experiment from spec text.
    pub fn from_text(
        text: &str,
        spec_dir: &Path,
        db: &CardDb,
        lib: &Library,
    ) -> Result<Self, ExperimentError> {
        let resolved = kiln_protocol::load_spec(text)?;
        Self::prepare(resolved, spec_dir, db, lib)
    }
}

/// Compiled playbooks by their path in the spec.
type Books = Arc<BTreeMap<String, Arc<Playbook>>>;

/// A policy from its recipe (M8 plan P4).
fn build_policy(p: &PolicySpec, books: &Books) -> Box<dyn Policy> {
    match p {
        PolicySpec::Random { pass } => Box::new(RandomPolicy::new(*pass)),
        PolicySpec::Passive => Box::new(PassivePolicy),
        PolicySpec::Playbook { playbook, fallback } => {
            let fallback = fallback.as_deref().map_or_else(
                || build_policy(&PolicySpec::Greedy, books),
                |f| build_policy(f, books),
            );
            match books.get(playbook) {
                Some(b) => Box::new(PlaybookPolicy::new(Arc::clone(b), fallback)),
                None => fallback,
            }
        }
        PolicySpec::Compose { chain } => Box::new(Compose::new(
            chain.iter().map(|q| build_policy(q, books)).collect(),
        )),
        PolicySpec::Search { budget, guide } => {
            let guide = guide.as_deref().cloned().unwrap_or(PolicySpec::Greedy);
            let name = build_policy(&guide, books).name();
            let books = Arc::clone(books);
            let make: PolicyFactory = Arc::new(move || build_policy(&guide, &books));
            Box::new(SearchPolicy::guided(
                SearchConfig {
                    iterations: budget.iterations,
                    rollout_depth: budget.rollout_depth,
                    max_ms: budget.ms,
                    ..SearchConfig::default()
                },
                name,
                make,
            ))
        }
        // Rejected by validation until M10.
        PolicySpec::Greedy | PolicySpec::Llm { .. } => Box::new(GreedyPolicy::default()),
    }
}

fn seat_id(seat: usize) -> PlayerId {
    PlayerId(u8::try_from(seat).unwrap_or(0))
}

/// Records players who have left the game since the last decision.
fn note_eliminations(game: &Game, record: &mut GameRecord) {
    for seat in 0..game.player_count() {
        if let PlayerStatus::Lost(r) = game.player_state(seat_id(seat)).status
            && !record.eliminations.iter().any(|e| e.seat == seat)
        {
            record.eliminations.push(Elimination {
                seat,
                turn: game.turn().number,
                reason: loss_name(r).into(),
            });
        }
    }
}

fn loss_name(r: LossReason) -> &'static str {
    match r {
        LossReason::ZeroLife => "life",
        LossReason::DrewFromEmptyLibrary => "decking",
        LossReason::Poison => "poison",
        LossReason::CommanderDamage => "commander_damage",
        LossReason::Conceded => "concession",
        LossReason::Effect => "effect_loss",
    }
}

/// M7 plan S11: an effect win while opponents remain is `alt_win`; otherwise
/// the way the last opponent left.
fn win_type(game: &Game, winner: PlayerId, record: &GameRecord) -> String {
    let others_in = (0..game.player_count())
        .map(seat_id)
        .any(|p| p != winner && game.player_state(p).status == PlayerStatus::Playing);
    if others_in {
        return "alt_win".into();
    }
    record
        .eliminations
        .last()
        .map_or_else(|| "last_standing".into(), |e| e.reason.clone())
}

/// The variant name of a decision kind ("ChooseTarget").
fn kind_name(k: &kiln_engine::DecisionKind) -> String {
    let s = format!("{k:?}");
    s.split([' ', '{', '(']).next().unwrap_or("").to_owned()
}

/// The `k`-th permutation of `0..n` in lexicographic order.
fn nth_permutation(n: usize, mut k: u64) -> Vec<usize> {
    let mut items: Vec<usize> = (0..n).collect();
    let mut out = Vec::with_capacity(n);
    for i in (1..=n).rev() {
        let f: u64 = (1..i as u64).product();
        let j = usize::try_from(k / f).unwrap_or(0);
        k %= f;
        out.push(items.remove(j));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::nth_permutation;

    #[test]
    fn permutations_are_lexicographic_and_complete() {
        assert_eq!(nth_permutation(3, 0), vec![0, 1, 2]);
        assert_eq!(nth_permutation(3, 1), vec![0, 2, 1]);
        assert_eq!(nth_permutation(3, 5), vec![2, 1, 0]);
        let mut all: Vec<Vec<usize>> = (0..24).map(|k| nth_permutation(4, k)).collect();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 24);
    }
}
