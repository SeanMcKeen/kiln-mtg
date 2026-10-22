//! Building a game in an arbitrary state, for scenario files and tests only
//! (`docs/08 Testing/Scenario File Format.md`).
//!
//! The Decision Model allows direct state setup for tests and scenarios and
//! nowhere else. A built game skips setup and mulligans and begins at the
//! chosen step, performing that step's turn-based actions first.

use std::sync::Arc;

use mtg_core::PlayerId;

use crate::card::{CardPool, DefId};
use crate::engine::{Game, GameRules, Task};
use crate::rng::GameRng;
use crate::state::{Arena, GameObject, ManaPool, Player, PlayerStatus, Step, TurnState, ZoneId};

/// Options for a permanent placed by the builder.
#[derive(Debug, Clone, Default)]
pub struct PermanentOpts {
    pub tapped: bool,
    pub damage: u32,
    /// Came under its controller's control this turn (CR 302.6).
    pub summoning_sick: bool,
    /// Counters already on it (CR 122.1).
    pub counters: Vec<(mtg_cards::ir::CounterKind, u32)>,
}

#[derive(Debug, Clone)]
struct SeatSpec {
    life: i64,
    /// Top first.
    library: Vec<DefId>,
    hand: Vec<DefId>,
    graveyard: Vec<DefId>,
    battlefield: Vec<(DefId, PermanentOpts)>,
    in_game: bool,
}

/// Builds a game state directly. Scenario and test use only.
#[derive(Debug, Clone)]
pub struct GameBuilder {
    pool: Arc<CardPool>,
    rules: GameRules,
    seats: Vec<SeatSpec>,
    turn: u32,
    active: PlayerId,
    step: Step,
    seed: u64,
}

impl GameBuilder {
    pub fn new(pool: Arc<CardPool>, players: usize) -> Self {
        let rules = GameRules::default();
        let seat = SeatSpec {
            life: rules.starting_life,
            library: Vec::new(),
            hand: Vec::new(),
            graveyard: Vec::new(),
            battlefield: Vec::new(),
            in_game: true,
        };
        Self {
            pool,
            rules,
            seats: vec![seat; players],
            turn: 3,
            active: PlayerId(0),
            step: Step::PrecombatMain,
            seed: 0,
        }
    }

    pub fn rules(mut self, rules: GameRules) -> Self {
        let life = rules.starting_life;
        for s in &mut self.seats {
            s.life = life;
        }
        self.rules = rules;
        self
    }

    pub fn life(mut self, p: PlayerId, life: i64) -> Self {
        self.seats[p.0 as usize].life = life;
        self
    }

    /// Library contents, top card first.
    pub fn library(mut self, p: PlayerId, cards: Vec<DefId>) -> Self {
        self.seats[p.0 as usize].library = cards;
        self
    }

    pub fn hand(mut self, p: PlayerId, cards: Vec<DefId>) -> Self {
        self.seats[p.0 as usize].hand = cards;
        self
    }

    pub fn graveyard(mut self, p: PlayerId, cards: Vec<DefId>) -> Self {
        self.seats[p.0 as usize].graveyard = cards;
        self
    }

    pub fn permanent(mut self, p: PlayerId, card: DefId, opts: PermanentOpts) -> Self {
        self.seats[p.0 as usize].battlefield.push((card, opts));
        self
    }

    /// The game starts at `step` of turn `number`, `active`'s turn.
    pub fn turn(mut self, number: u32, active: PlayerId, step: Step) -> Self {
        self.turn = number.max(1);
        self.active = active;
        self.step = step;
        self
    }

    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Builds the game and runs it to the first decision.
    pub fn build(self) -> Game {
        let n = self.seats.len();
        let mut objects = Arena::default();
        let mut players = Vec::with_capacity(n);
        let mut battlefield = Vec::new();
        let mut counters = Vec::new();
        for (i, seat) in self.seats.iter().enumerate() {
            let id = PlayerId(u8::try_from(i).unwrap_or(0));
            let mut make = |def: DefId, zone: ZoneId, opts: &PermanentOpts| {
                objects.alloc(GameObject {
                    def,
                    owner: id,
                    controller: id,
                    zone,
                    tapped: opts.tapped,
                    damage: opts.damage,
                    deathtouch_damage: false,
                    control_since: if opts.summoning_sick { self.turn } else { 0 },
                })
            };
            // Library vectors keep the top card last.
            let library = self.seats[i]
                .library
                .iter()
                .rev()
                .map(|d| make(*d, ZoneId::Library(id), &PermanentOpts::default()))
                .collect();
            let hand = seat
                .hand
                .iter()
                .map(|d| make(*d, ZoneId::Hand(id), &PermanentOpts::default()))
                .collect();
            let graveyard = seat
                .graveyard
                .iter()
                .map(|d| make(*d, ZoneId::Graveyard(id), &PermanentOpts::default()))
                .collect();
            for (d, opts) in &seat.battlefield {
                let obj = make(*d, ZoneId::Battlefield, opts);
                counters.extend(
                    opts.counters
                        .iter()
                        .filter(|(_, n)| *n > 0)
                        .map(|(k, n)| (obj, *k, *n)),
                );
                battlefield.push(obj);
            }
            // Every player has had a turn already (or is having it), so permanents
            // placed without `summoning_sick` aren't sick (CR 302.6).
            let last_turn = if id == self.active {
                self.turn
            } else {
                self.turn.saturating_sub(1).max(1)
            };
            players.push(Player {
                id,
                life: seat.life,
                poison: 0,
                pool: ManaPool::default(),
                library,
                hand,
                graveyard,
                lands_played: 0,
                status: if seat.in_game {
                    PlayerStatus::Playing
                } else {
                    PlayerStatus::Lost(crate::state::LossReason::Conceded)
                },
                drew_from_empty: false,
                mulligans: 0,
                kept: true,
                last_turn,
            });
        }
        let mut game = Game {
            pool: self.pool,
            rules: self.rules,
            players,
            objects,
            battlefield,
            stack: Vec::new(),
            exile: Vec::new(),
            turn: TurnState {
                number: self.turn,
                active: self.active,
                step: self.step,
                priority: None,
                passes: 0,
                extra_cleanup: false,
            },
            starting_player: PlayerId(0),
            combat: None,
            rng: GameRng::new(self.seed),
            tasks: vec![Task::BeginStep],
            pending: None,
            next_decision: 0,
            result: None,
            events: Vec::new(),
            event_count: 0,
            pt_effects: Vec::new(),
            activated_this_turn: Vec::new(),
            counters,
            pending_triggers: Vec::new(),
            leaving_batch: Vec::new(),
            loops: crate::loops::LoopTracker::default(),
            linked: Vec::new(),
            copies: Vec::new(),
            face_down: Vec::new(),
            delayed: Vec::new(),
            cast_this_turn: Vec::new(),
            combat_damaged: 0,
            drew_in_draw_step: false,
        };
        if matches!(
            self.step,
            Step::DeclareAttackers
                | Step::DeclareBlockers
                | Step::FirstStrikeDamage
                | Step::CombatDamage
                | Step::EndCombat
        ) {
            // Combat steps need a combat in progress (CR 506).
            game.combat = Some(crate::state::Combat::default());
        }
        game.run();
        game
    }
}
