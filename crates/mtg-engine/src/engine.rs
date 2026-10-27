//! The game: setup, the task-stack loop, and the decision boundary.
//!
//! Rules procedures are a stack of [`Task`]s stored in the state (M2 plan, D3).
//! [`Game::run`] pops and executes tasks until one needs a decision; then
//! [`Game::apply`] hands the choice to the task that asked. Because the whole
//! procedure position is plain data, a game can be cloned or serialized at
//! any decision (ADR-0004).

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use mtg_cards::ir;
use mtg_core::{CardType, CrRef, ObjectId, PlayerId};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::card::{CardDef, CardPool, DefId, Keywords};
use crate::decision::{Action, Choice, ChoiceError, DecisionId, DecisionKind, PendingDecision};
use crate::effects::{AbilityRef, EffectCtx, EffectTask};
use crate::events::Event;
use crate::rng::GameRng;
use crate::state::{
    Arena, Combat, GameObject, LossReason, Player, PlayerStatus, StableHasher, StackItem, Step,
    Target, TurnState, ZoneId,
};

/// Rules and harness options for one game.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GameRules {
    /// CR 103.4 (20), 103.4c (40 in Commander).
    pub starting_life: i64,
    /// CR 103.5: normally seven.
    pub starting_hand_size: u32,
    /// CR 402.2: normally seven.
    pub max_hand_size: u32,
    /// Harness limit, not a rule (M2 plan, D10): the game is aborted after this many turns.
    pub turn_limit: Option<u32>,
    /// Record the event log. Off for maximum throughput.
    pub log_events: bool,
}

impl GameRules {
    /// Commander rules: 40 starting life (CR 103.4c).
    pub fn commander() -> Self {
        Self {
            starting_life: 40,
            ..Self::default()
        }
    }
}

impl Default for GameRules {
    fn default() -> Self {
        Self {
            starting_life: 20,
            starting_hand_size: 7,
            max_hand_size: 7,
            turn_limit: Some(500),
            log_events: true,
        }
    }
}

/// Everything needed to start a game deterministically.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GameSetup {
    pub rules: GameRules,
    /// One deck per seat, in seat order.
    pub decks: Vec<Vec<DefId>>,
    pub seed: u64,
    /// CR 103.1: who plays first. `None` = chosen at random with the game RNG.
    pub first_player: Option<PlayerId>,
    /// Each seat's commanders (CR 903.3), not part of the deck list; empty
    /// outside Commander games.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commanders: Vec<Vec<DefId>>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SetupError {
    #[error("a game needs at least 2 players, got {0}")]
    TooFewPlayers(usize),
    #[error("at most 8 players are supported, got {0}")]
    TooManyPlayers(usize),
    #[error("deck {seat} contains unknown definition {def:?}")]
    UnknownDef { seat: usize, def: DefId },
    #[error("first player {0} isn't a seat in this game")]
    BadFirstPlayer(PlayerId),
    #[error("seat {seat} has {count} commanders; at most 2 are allowed (CR 903.3, 702.124)")]
    TooManyCommanders { seat: usize, count: usize },
}

/// Why a game ended without a rules result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AbortReason {
    TurnLimit,
}

/// How a game ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameResult {
    /// CR 104.2a
    Win {
        player: PlayerId,
        turn: u32,
    },
    /// CR 104.4a
    Draw {
        turn: u32,
    },
    Aborted {
        reason: AbortReason,
        turn: u32,
    },
}

/// A suspended rules procedure (M2 plan, D3).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Task {
    /// CR 103.5: draw opening hands.
    DrawOpeningHands,
    /// CR 103.5: one round of mulligan declarations.
    MulliganRound,
    MulliganDeclare {
        order: SmallVec<[PlayerId; 8]>,
        next: usize,
        takers: SmallVec<[PlayerId; 8]>,
    },
    MulliganBottom {
        player: PlayerId,
        remaining: u32,
    },
    BeginTurn,
    BeginStep,
    /// CR 117.5: `checked` = SBAs already done for this grant.
    Priority {
        checked: bool,
    },
    EndStep,
    Resolve,
    Cast(crate::cast::CastTask),
    ManualPay(crate::cast::ManualPayTask),
    DeclareAttackers(crate::combat::AttackTask),
    DeclareBlockers(crate::combat::BlockTask),
    CombatDamage(crate::combat::DamageTask),
    Cleanup {
        discarded: bool,
    },
    Sba(crate::sba::SbaTask),
    /// An effect being executed (M3 plan, B-D3).
    Effect(Box<crate::effects::EffectTask>),
    /// CR 608.2n: the last step of an instant or sorcery's resolution.
    FinishSpell {
        object: ObjectId,
    },
    /// The last step of a payment plan: spend mana from the pool (CR 601.2h).
    SpendMana {
        player: PlayerId,
        spend: [u32; 6],
        /// The spell being cast, whose restricted mana may be spent (CR 106.6).
        spell: Option<ObjectId>,
    },
    /// CR 608.2n: the last step of an ability's resolution.
    FinishAbility {
        source: ObjectId,
        ability: u8,
    },
    /// CR 603.3: put pending triggered abilities on the stack.
    StackTriggers(crate::triggers::StackTriggersTask),
    /// Draw a card that a replacement effect may change (CR 614.11).
    Draw(crate::replacement::DrawTask),
    /// Proposing and answering a shortcut for a detected loop (CR 732.2).
    Shortcut(crate::loops::ShortcutTask),
    /// CR 103.6: opening-hand actions, the starting player first, then each
    /// other player in turn order.
    Pregame {
        order: SmallVec<[PlayerId; 8]>,
        next: usize,
    },
}

/// A game in progress (or finished).
#[derive(Debug, Clone)]
pub struct Game {
    pub(crate) pool: Arc<CardPool>,
    pub(crate) rules: GameRules,
    pub(crate) players: Vec<Player>,
    pub(crate) objects: Arena,
    pub(crate) battlefield: Vec<ObjectId>,
    pub(crate) stack: Vec<StackItem>,
    pub(crate) exile: Vec<ObjectId>,
    pub(crate) turn: TurnState,
    pub(crate) starting_player: PlayerId,
    pub(crate) combat: Option<Combat>,
    pub(crate) rng: GameRng,
    pub(crate) tasks: Vec<Task>,
    pub(crate) pending: Option<PendingDecision>,
    pub(crate) next_decision: u64,
    pub(crate) result: Option<GameResult>,
    pub(crate) events: Vec<Event>,
    pub(crate) event_count: u64,
    /// Until-end-of-turn power/toughness changes (layer 7c, CR 613.4c), ended in cleanup (CR 514.2).
    /// Continuous effects from resolved spells and abilities (CR 611.2).
    pub(crate) effects: Vec<crate::layers::ContinuousEffect>,
    /// The latest timestamp handed out (CR 613.7).
    pub(crate) timestamp: u64,
    /// Bumped whenever characteristics may have changed (not hashed).
    pub(crate) layer_epoch: u64,
    /// Computed characteristics for the current epoch (not hashed).
    pub(crate) layer_cache: crate::layers::LayerCache,
    /// "Activate only once each turn" abilities activated this turn (CR 602.5b).
    pub(crate) activated_this_turn: Vec<(ObjectId, u8)>,
    /// Counters on objects (CR 122), sorted by object and kind.
    pub(crate) counters: Vec<(ObjectId, ir::CounterKind, u32)>,
    /// Triggered abilities waiting to be put on the stack (CR 603.3).
    pub(crate) pending_triggers: Vec<crate::triggers::PendingTrigger>,
    /// Permanents leaving the battlefield in the current simultaneous batch
    /// (CR 603.10a look-back). Empty except while a batch moves.
    pub(crate) leaving_batch: Vec<(ObjectId, DefId, PlayerId)>,
    /// Loop detection (CR 732); not hashed, like the event log.
    pub(crate) loops: crate::loops::LoopTracker,
    /// Objects an ability of a permanent exiled and remembered, linked to
    /// that permanent's other abilities (CR 607.2a): (source, name, objects).
    pub(crate) linked: Vec<(ObjectId, String, SmallVec<[ObjectId; 2]>)>,
    /// Tokens that have left the battlefield (CR 704.5d candidates). Derived
    /// from the objects, so not hashed; tokens are only ever created on the
    /// battlefield, so a token elsewhere is always in this list.
    pub(crate) tokens_off_field: Vec<ObjectId>,
    /// Phased-out permanents and who controlled them as they phased out
    /// (CR 702.26): kept out of `battlefield`, so the game treats them as
    /// though they don't exist; their zone is still the battlefield.
    pub(crate) phased_out: Vec<(ObjectId, PlayerId)>,
    /// Player rules until a player's next turn: (affected, rule, ends as this
    /// player's next turn begins).
    pub(crate) player_rules: Vec<(PlayerId, mtg_cards::ir::PlayerRuleKind, PlayerId)>,
    /// Attachments (CR 303.4, 301.5): (attached object, host, bound to the
    /// object its own effect put onto the battlefield).
    pub(crate) attachments: Vec<(ObjectId, Target, bool)>,
    /// Spells being cast with mana spent as though it were any color
    /// (Opposition Agent).
    pub(crate) any_color_spells: Vec<ObjectId>,
    /// Copies of cards (CR 707.12); they cease to exist outside the stack and
    /// battlefield (CR 704.5e).
    pub(crate) copies: Vec<ObjectId>,
    /// Cards exiled face down (CR 406.3): no player may look at them.
    pub(crate) face_down: Vec<ObjectId>,
    /// Delayed triggered abilities waiting for their event (CR 603.7).
    pub(crate) delayed: Vec<crate::triggers::DelayedTrigger>,
    /// Spells cast this turn, by caster ("first ... each turn"; tracked only
    /// when the pool has triggers).
    pub(crate) cast_this_turn: Vec<crate::state::CastRecord>,
    /// Bitmask of players dealt combat damage this turn (tracked only when
    /// the pool has triggers).
    pub(crate) combat_damaged: u8,
    /// The active player has drawn in this turn's draw step (tracked only
    /// when the pool has triggers; see `draw`).
    pub(crate) drew_in_draw_step: bool,
    /// Commanders and the command zone (CR 903); empty outside Commander games.
    pub(crate) cmd: crate::commander::CommanderState,
    /// Restricted mana in pools (CR 106.6), and spells made uncounterable by
    /// the mana spent on them.
    pub(crate) restricted: Vec<crate::state::RestrictedMana>,
    pub(crate) uncounterable: Vec<ObjectId>,
    /// Choices made as permanents entered (CR 614.12): (object, name, value;
    /// a color is its index in `Color::ALL`). Recorded for the card about to
    /// enter, then carried to the permanent. A move onto the battlefield
    /// without the choice counts in `unasked_entering` (invariant 11).
    pub(crate) object_vars: Vec<(ObjectId, String, i64)>,
    pub(crate) unasked_entering: u32,
    /// Spells on the stack whose kicker was paid (CR 702.33).
    pub(crate) kicked: Vec<ObjectId>,
    /// Player rules until end of turn (Veil of Summer, Angel's Grace).
    pub(crate) player_rules_eot: Vec<(PlayerId, mtg_cards::ir::PlayerRuleKind)>,
    /// "You may cast that card this turn": (card, player).
    pub(crate) cast_permissions: Vec<(ObjectId, PlayerId)>,
    /// Spells being cast from a graveyard with escape and the extra cost
    /// their escape grant adds (CR 702.138a; Underworld Breach).
    pub(crate) escaping: Vec<(ObjectId, Vec<ir::CostPart>)>,
    /// Spells being cast from the top of a library with Bolas's Citadel.
    pub(crate) library_casts: Vec<ObjectId>,
    /// Additional combat phases still to come this turn (CR 500.8; Najeela).
    pub(crate) extra_combats: u8,
    /// What Auras entering without being cast will enchant (CR 303.4f).
    pub(crate) entering_hosts: Vec<(ObjectId, Target)>,
    /// Cards drawn this turn, by player (Faerie Mastermind's "second card",
    /// Sylvan Library's "drawn this turn").
    pub(crate) drawn_this_turn: Vec<(PlayerId, ObjectId)>,
    /// Spells being cast from a graveyard, and whether by a permission (the
    /// mana cost is then allowed, not only escape).
    pub(crate) graveyard_casts: Vec<(ObjectId, bool)>,
}

impl Game {
    /// Sets up a game (CR 103) and runs it to the first decision.
    pub fn new(pool: Arc<CardPool>, setup: &GameSetup) -> Result<Game, SetupError> {
        let n = setup.decks.len();
        if n < 2 {
            return Err(SetupError::TooFewPlayers(n));
        }
        if n > 8 {
            return Err(SetupError::TooManyPlayers(n));
        }
        for (seat, deck) in setup.decks.iter().chain(&setup.commanders).enumerate() {
            if let Some(def) = deck.iter().find(|d| d.0 as usize >= pool.len()) {
                return Err(SetupError::UnknownDef {
                    seat: seat % n,
                    def: *def,
                });
            }
        }
        for (seat, c) in setup.commanders.iter().enumerate() {
            if c.len() > 2 {
                return Err(SetupError::TooManyCommanders {
                    seat,
                    count: c.len(),
                });
            }
        }
        let mut rng = GameRng::new(setup.seed);
        let starting_player = match setup.first_player {
            Some(p) if (p.0 as usize) < n => p,
            Some(p) => return Err(SetupError::BadFirstPlayer(p)),
            // CR 103.1: a random method chooses who plays first.
            None => PlayerId(u8::try_from(rng.index(n)).unwrap_or(0)),
        };
        let mut game = Game {
            pool,
            rules: setup.rules.clone(),
            players: Vec::with_capacity(n),
            objects: Arena::default(),
            battlefield: Vec::new(),
            stack: Vec::new(),
            exile: Vec::new(),
            turn: TurnState {
                number: 0,
                active: starting_player,
                step: Step::Untap,
                priority: None,
                passes: 0,
                extra_cleanup: false,
            },
            starting_player,
            combat: None,
            rng,
            tasks: Vec::new(),
            pending: None,
            next_decision: 0,
            result: None,
            events: Vec::new(),
            event_count: 0,
            effects: Vec::new(),
            timestamp: 0,
            layer_epoch: 0,
            layer_cache: crate::layers::LayerCache::default(),
            activated_this_turn: Vec::new(),
            counters: Vec::new(),
            pending_triggers: Vec::new(),
            leaving_batch: Vec::new(),
            loops: crate::loops::LoopTracker::default(),
            tokens_off_field: Vec::new(),
            any_color_spells: Vec::new(),
            phased_out: Vec::new(),
            player_rules: Vec::new(),
            attachments: Vec::new(),
            linked: Vec::new(),
            copies: Vec::new(),
            face_down: Vec::new(),
            delayed: Vec::new(),
            cast_this_turn: Vec::new(),
            combat_damaged: 0,
            drew_in_draw_step: false,
            cmd: crate::commander::CommanderState::default(),
            restricted: Vec::new(),
            uncounterable: Vec::new(),
            object_vars: Vec::new(),
            unasked_entering: 0,
            kicked: Vec::new(),
            player_rules_eot: Vec::new(),
            cast_permissions: Vec::new(),
            escaping: Vec::new(),
            graveyard_casts: Vec::new(),
            drawn_this_turn: Vec::new(),
            library_casts: Vec::new(),
            extra_combats: 0,
            entering_hosts: Vec::new(),
        };
        // CR 903.6: each commander starts the game in the command zone.
        for (seat, defs) in setup.commanders.iter().enumerate() {
            let owner = PlayerId(u8::try_from(seat).unwrap_or(0));
            for &def in defs {
                let id = game.objects.alloc(GameObject {
                    def,
                    owner,
                    controller: owner,
                    zone: ZoneId::Command,
                    tapped: false,
                    damage: 0,
                    deathtouch_damage: false,
                    control_since: 0,
                    timestamp: 0,
                });
                game.cmd.zone.push(id);
                game.cmd.commanders.push(crate::commander::Commander {
                    owner,
                    def,
                    object: Some(id),
                    casts: 0,
                    damage: vec![0; n],
                });
            }
        }
        for (seat, deck) in setup.decks.iter().enumerate() {
            let id = PlayerId(u8::try_from(seat).unwrap_or(0));
            // CR 103.3: decks are shuffled to become libraries. Objects are
            // allocated in library order so ids don't reveal decklist order.
            let mut cards = deck.clone();
            game.rng.shuffle(&mut cards);
            let mut library = Vec::with_capacity(cards.len());
            for def in cards {
                library.push(game.objects.alloc(GameObject {
                    def,
                    owner: id,
                    controller: id,
                    zone: ZoneId::Library(id),
                    tapped: false,
                    damage: 0,
                    deathtouch_damage: false,
                    control_since: 0,
                    timestamp: 0,
                }));
            }
            game.players.push(Player {
                id,
                life: setup.rules.starting_life,
                poison: 0,
                pool: crate::state::ManaPool::default(),
                library,
                hand: Vec::new(),
                graveyard: Vec::new(),
                lands_played: 0,
                status: PlayerStatus::Playing,
                drew_from_empty: false,
                mulligans: 0,
                kept: false,
                last_turn: 0,
            });
        }
        game.log(Event::GameStarted { starting_player });
        game.tasks.push(Task::BeginTurn);
        if game.pool.iter().any(|(_, d)| d.opening_hand().is_some()) {
            let order = game.turn_order_from(starting_player).into_iter().collect();
            game.tasks.push(Task::Pregame { order, next: 0 });
        }
        game.tasks.push(Task::MulliganRound);
        game.tasks.push(Task::DrawOpeningHands);
        game.run();
        Ok(game)
    }

    // ---- public API -------------------------------------------------------

    /// The decision waiting for an answer; `None` once the game is over.
    pub fn pending(&self) -> Option<&PendingDecision> {
        self.pending.as_ref()
    }

    pub fn result(&self) -> Option<&GameResult> {
        self.result.as_ref()
    }

    pub fn is_over(&self) -> bool {
        self.result.is_some()
    }

    pub fn card_pool(&self) -> &CardPool {
        &self.pool
    }

    pub fn rules(&self) -> &GameRules {
        &self.rules
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn turn(&self) -> &TurnState {
        &self.turn
    }

    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    pub fn starting_player(&self) -> PlayerId {
        self.starting_player
    }

    // Omniscient read access for tests, scenarios and judges. Policies never
    // get a `Game`; they get a `PlayerView` (Design Principle 5).

    /// A player's full state.
    pub fn player_state(&self, p: PlayerId) -> &Player {
        self.player(p)
    }

    /// A live object.
    pub fn object(&self, id: ObjectId) -> Option<&GameObject> {
        self.obj(id)
    }

    /// The definition of a live object.
    pub fn object_def(&self, id: ObjectId) -> Option<&CardDef> {
        self.obj_def(id)
    }

    /// An object's name now: on the battlefield a copy has the copied name
    /// (CR 707.2, 613); elsewhere its printed name.
    pub fn object_name(&self, id: ObjectId) -> Option<&str> {
        let def = self.chars(id)?.def_id();
        Some(self.def(def).name.as_str())
    }

    /// Objects in a zone, in zone order (libraries and graveyards: bottom first).
    pub fn objects_in(&self, zone: ZoneId) -> Vec<ObjectId> {
        match zone {
            ZoneId::Library(p) => self.player(p).library.clone(),
            ZoneId::Hand(p) => self.player(p).hand.clone(),
            ZoneId::Graveyard(p) => self.player(p).graveyard.clone(),
            ZoneId::Battlefield => self.battlefield.clone(),
            ZoneId::Exile => self.exile.clone(),
            ZoneId::Stack => self
                .stack
                .iter()
                .filter(|s| s.is_spell())
                .map(|s| s.object)
                .collect(),
            ZoneId::Command => self.cmd.zone.clone(),
        }
    }

    /// The stack, bottom first.
    pub fn stack_items(&self) -> &[StackItem] {
        &self.stack
    }

    /// Answers the pending decision. The choice is re-validated; anything not
    /// offered is rejected without changing the game.
    pub fn apply(&mut self, choice: Choice) -> Result<(), ChoiceError> {
        let Some(pending) = &self.pending else {
            return Err(ChoiceError::NoDecision);
        };
        if pending.id != choice.decision {
            return Err(ChoiceError::WrongDecision {
                expected: pending.id,
                got: choice.decision,
            });
        }
        let count = pending.option_count();
        if choice.option as usize >= count {
            return Err(ChoiceError::OutOfRange {
                decision: pending.id,
                option: choice.option,
                count,
            });
        }
        let decision = self.pending.take().unwrap_or_else(|| unreachable!());
        // A loop of only passes is mandatory (CR 732.4); count everything else.
        // A decision with one option isn't a choice either.
        let pass = matches!(decision.kind, DecisionKind::Priority { .. }) && choice.option == 0;
        if !pass
            && count > 1
            && let Some(c) = self.loops.choices.get_mut(decision.player.0 as usize)
        {
            *c += 1;
        }
        let task = self
            .tasks
            .pop()
            .unwrap_or_else(|| unreachable!("a pending decision always has its task on top"));
        self.answer(task, decision, choice.option as usize);
        self.run();
        Ok(())
    }

    /// Answers the pending decision by option index.
    pub fn apply_option(&mut self, option: u32) -> Result<(), ChoiceError> {
        let Some(pending) = &self.pending else {
            return Err(ChoiceError::NoDecision);
        };
        let decision = pending.id;
        self.apply(Choice { decision, option })
    }

    /// Deterministic hash of the game-relevant state (CR-relevant fields, RNG
    /// position, procedure stack, pending decision). Used for replays and the
    /// determinism invariant.
    pub fn state_hash(&self) -> u64 {
        let mut h = StableHasher::default();
        self.rules.hash(&mut h);
        self.players.hash(&mut h);
        self.objects.hash(&mut h);
        self.battlefield.hash(&mut h);
        self.stack.hash(&mut h);
        self.exile.hash(&mut h);
        self.turn.hash(&mut h);
        self.starting_player.hash(&mut h);
        self.combat.hash(&mut h);
        self.rng.draws().hash(&mut h);
        self.tasks.hash(&mut h);
        self.pending.hash(&mut h);
        self.result.hash(&mut h);
        // State added after M2 is hashed only when present, so states that
        // don't use it keep their M2 hashes (golden replays stay comparable).
        if !self.effects.is_empty() {
            self.effects.hash(&mut h);
        }
        self.timestamp.hash(&mut h);
        if !self.activated_this_turn.is_empty() {
            self.activated_this_turn.hash(&mut h);
        }
        if !self.counters.is_empty() {
            self.counters.hash(&mut h);
        }
        if !self.pending_triggers.is_empty() {
            self.pending_triggers.hash(&mut h);
        }
        if self.drew_in_draw_step {
            self.drew_in_draw_step.hash(&mut h);
        }
        if !self.delayed.is_empty() {
            self.delayed.hash(&mut h);
        }
        if !self.face_down.is_empty() {
            self.face_down.hash(&mut h);
        }
        if !self.linked.is_empty() {
            self.linked.hash(&mut h);
        }
        if !self.copies.is_empty() {
            self.copies.hash(&mut h);
        }
        if !self.any_color_spells.is_empty() {
            self.any_color_spells.hash(&mut h);
        }
        if !self.attachments.is_empty() {
            self.attachments.hash(&mut h);
        }
        if !self.phased_out.is_empty() {
            self.phased_out.hash(&mut h);
        }
        if !self.player_rules.is_empty() {
            self.player_rules.hash(&mut h);
        }
        if !self.cast_this_turn.is_empty() {
            self.cast_this_turn.hash(&mut h);
        }
        if self.combat_damaged != 0 {
            self.combat_damaged.hash(&mut h);
        }
        if !self.cmd.is_empty() {
            self.cmd.hash(&mut h);
        }
        if !self.restricted.is_empty() {
            self.restricted.hash(&mut h);
        }
        if !self.uncounterable.is_empty() {
            self.uncounterable.hash(&mut h);
        }
        if !self.object_vars.is_empty() {
            self.object_vars.hash(&mut h);
        }
        if !self.kicked.is_empty() {
            self.kicked.hash(&mut h);
        }
        if !self.player_rules_eot.is_empty() {
            self.player_rules_eot.hash(&mut h);
        }
        if !self.cast_permissions.is_empty() {
            self.cast_permissions.hash(&mut h);
        }
        if !self.escaping.is_empty() {
            self.escaping.hash(&mut h);
        }
        if !self.graveyard_casts.is_empty() {
            self.graveyard_casts.hash(&mut h);
        }
        if !self.drawn_this_turn.is_empty() {
            self.drawn_this_turn.hash(&mut h);
        }
        if !self.library_casts.is_empty() {
            self.library_casts.hash(&mut h);
        }
        if self.extra_combats > 0 {
            self.extra_combats.hash(&mut h);
        }
        if !self.entering_hosts.is_empty() {
            self.entering_hosts.hash(&mut h);
        }
        h.finish()
    }

    // ---- read helpers ------------------------------------------------------

    pub(crate) fn def(&self, def: DefId) -> &CardDef {
        self.pool.get(def)
    }

    pub(crate) fn obj(&self, id: ObjectId) -> Option<&GameObject> {
        self.objects.get(id)
    }

    pub(crate) fn obj_def(&self, id: ObjectId) -> Option<&CardDef> {
        self.objects.get(id).map(|o| self.pool.get(o.def))
    }

    pub(crate) fn player(&self, p: PlayerId) -> &Player {
        &self.players[p.0 as usize]
    }

    pub(crate) fn player_mut(&mut self, p: PlayerId) -> &mut Player {
        &mut self.players[p.0 as usize]
    }

    pub(crate) fn in_game(&self, p: PlayerId) -> bool {
        self.player(p).in_game()
    }

    pub(crate) fn players_in_game(&self) -> usize {
        self.players.iter().filter(|p| p.in_game()).count()
    }

    /// Seats in turn order starting with `from` (inclusive), in-game players only.
    pub(crate) fn turn_order_from(&self, from: PlayerId) -> SmallVec<[PlayerId; 8]> {
        let n = self.players.len();
        (0..n)
            .map(|i| PlayerId(u8::try_from((from.0 as usize + i) % n).unwrap_or(0)))
            .filter(|p| self.in_game(*p))
            .collect()
    }

    /// The next in-game player after `p` in turn order.
    pub(crate) fn next_in_game(&self, p: PlayerId) -> PlayerId {
        let n = self.players.len();
        (1..=n)
            .map(|i| PlayerId(u8::try_from((p.0 as usize + i) % n).unwrap_or(0)))
            .find(|q| self.in_game(*q))
            .unwrap_or(p)
    }

    /// CR 302.6: under its controller's control continuously since their most recent turn began.
    pub(crate) fn is_summoning_sick(&self, id: ObjectId) -> bool {
        let Some(o) = self.obj(id) else { return true };
        o.control_since >= self.player(o.controller).last_turn
    }

    // ---- mutation primitives ----------------------------------------------

    pub(crate) fn log(&mut self, e: Event) {
        self.event_count += 1;
        // CR 603.2: events trigger abilities whether or not they're logged.
        if self.pool.has_triggers() {
            self.match_triggers(&e);
        }
        if self.rules.log_events {
            self.events.push(e);
        }
    }

    pub(crate) fn decide(&mut self, player: PlayerId, kind: DecisionKind, cr: &str) {
        let id = DecisionId(self.next_decision);
        self.next_decision += 1;
        let cr: CrRef = cr
            .parse()
            .unwrap_or_else(|e| panic!("engine CR literal {cr:?}: {e}"));
        self.pending = Some(PendingDecision {
            id,
            player,
            kind,
            cr,
        });
    }

    pub(crate) fn zone_vec_mut(&mut self, zone: ZoneId) -> &mut Vec<ObjectId> {
        match zone {
            ZoneId::Library(p) => &mut self.players[p.0 as usize].library,
            ZoneId::Hand(p) => &mut self.players[p.0 as usize].hand,
            ZoneId::Graveyard(p) => &mut self.players[p.0 as usize].graveyard,
            ZoneId::Battlefield => &mut self.battlefield,
            ZoneId::Exile => &mut self.exile,
            ZoneId::Command => &mut self.cmd.zone,
            ZoneId::Stack => unreachable!("the stack is handled separately"),
        }
    }

    /// Removes `id` from its zone list (not from the arena).
    pub(crate) fn detach_object(&mut self, id: ObjectId, zone: ZoneId) {
        self.detach(id, zone);
    }

    fn detach(&mut self, id: ObjectId, zone: ZoneId) {
        if zone == ZoneId::Stack {
            self.stack.retain(|s| !(s.is_spell() && s.object == id));
        } else {
            self.zone_vec_mut(zone).retain(|x| *x != id);
        }
        if zone == ZoneId::Battlefield {
            if let Some(c) = &mut self.combat {
                c.remove(id);
            }
            if !self.phased_out.is_empty() {
                self.phased_out.retain(|(x, _)| *x != id);
            }
        }
    }

    /// The zone-change primitive (CR 400.7): the object leaves its zone and a
    /// new object enters `to`. `top` puts it on top of an ordered zone,
    /// otherwise on the bottom. Returns the new id.
    pub(crate) fn move_object(&mut self, id: ObjectId, to: ZoneId, top: bool) -> Option<ObjectId> {
        self.move_object_with(id, to, top, |_, _| {})
    }

    /// [`Game::move_object`], running `entering` on the new object before the
    /// zone change is logged (an "enters as a copy" effect must exist as it
    /// enters, CR 614.12, so its abilities can trigger).
    pub(crate) fn move_object_with(
        &mut self,
        id: ObjectId,
        to: ZoneId,
        top: bool,
        entering: impl FnOnce(&mut Self, ObjectId),
    ) -> Option<ObjectId> {
        let old = *self.objects.get(id)?;
        // CR 111.8: a token that has left the battlefield stays where it is.
        if !self.tokens_off_field.is_empty() && self.tokens_off_field.contains(&id) {
            return None;
        }
        // CR 903.9b: the owner may have chosen the command zone instead.
        let to = self.commander_destination(id, to);
        // CR 614.1a: mandatory "exile it instead" replacements.
        let to = self.zone_replacement(id, old.zone, to);
        // Last known information (CR 603.10, 608.2h) before the object leaves,
        // including what it was copying (CR 707.2).
        let (power, toughness) = (self.power(id), self.toughness(id));
        let lki_def = self.chars(id).map_or(old.def, |c| c.def_id());
        self.detach(id, old.zone);
        if old.zone == ZoneId::Exile && !self.face_down.is_empty() {
            self.face_down.retain(|x| *x != id);
        }
        let was_copy = !self.copies.is_empty() && self.copies.contains(&id);
        if was_copy {
            self.copies.retain(|x| *x != id);
        }
        self.objects.free(id);
        self.drop_counters(id);
        let controller = match to {
            ZoneId::Library(p) | ZoneId::Hand(p) | ZoneId::Graveyard(p) => p,
            _ => old.controller,
        };
        // CR 614.1c-d, 614.12: "enters tapped" modifies the event itself.
        let tapped = to == ZoneId::Battlefield
            && self.pool.has_replacements()
            && self.enters_tapped(old.def, controller);
        let timestamp = self.next_timestamp();
        self.bump_layers();
        let new = self.objects.alloc(GameObject {
            def: old.def,
            owner: old.owner,
            controller,
            zone: to,
            tapped,
            damage: 0,
            deathtouch_damage: false,
            control_since: self.turn.number,
            timestamp,
        });
        if to != ZoneId::Stack {
            let v = self.zone_vec_mut(to);
            if top { v.push(new) } else { v.insert(0, new) }
        }
        self.commander_moved(id, Some(new), to);
        if !self.uncounterable.is_empty() {
            self.uncounterable.retain(|x| *x != id);
        }
        if !self.kicked.is_empty() {
            self.kicked.retain(|x| *x != id);
        }
        if !self.cast_permissions.is_empty() {
            self.cast_permissions.retain(|(x, _)| *x != id);
        }
        if !self.escaping.is_empty() {
            self.escaping.retain(|(x, _)| *x != id);
        }
        if !self.graveyard_casts.is_empty() {
            self.graveyard_casts.retain(|(x, _)| *x != id);
        }
        if !self.library_casts.is_empty() {
            self.library_casts.retain(|x| *x != id);
        }
        // CR 614.12: an "as this enters" choice goes with it onto the
        // battlefield; anything else forgets it (CR 400.7).
        if to == ZoneId::Battlefield && self.def(old.def).as_enters().is_some() {
            let mut carried = false;
            for v in &mut self.object_vars {
                if v.0 == id {
                    v.0 = new;
                    carried = true;
                }
            }
            if !carried {
                self.unasked_entering += 1;
            }
        } else if !self.object_vars.is_empty() {
            self.object_vars.retain(|v| v.0 != id);
        }
        // A copy stays a copy as it moves (it's cast from exile, CR 707.12).
        if was_copy {
            self.copies.push(new);
        }
        if to != ZoneId::Battlefield && self.def(old.def).token {
            self.tokens_off_field.push(new);
        }
        entering(self, new);
        // CR 306.5b: a planeswalker enters with its printed loyalty (a
        // copy's, since the copy effect applies as it enters, CR 707.2).
        if to == ZoneId::Battlefield {
            let def = self.chars(new).map_or(old.def, |c| c.def_id());
            if let Some(n) = self.def(def).loyalty
                && self.def(def).is(CardType::Planeswalker)
            {
                self.add_counters(
                    new,
                    ir::CounterKind::Loyalty,
                    u32::try_from(n.max(0)).unwrap_or(0),
                );
            }
        }
        self.log(Event::ZoneChange {
            def: lki_def,
            owner: old.owner,
            controller: old.controller,
            power,
            toughness,
            from: old.zone,
            to,
            old: id,
            new,
        });
        Some(new)
    }

    /// CR 121.1: draw a card. Drawing from an empty library flags the
    /// player for CR 704.5b.
    pub(crate) fn draw(&mut self, p: PlayerId) {
        let Some(&top) = self.player(p).library.last() else {
            self.player_mut(p).drew_from_empty = true;
            return;
        };
        // Tracked only when something could care ("except the first one they
        // draw in each of their draw steps"), so trigger-free games keep M2 state.
        let first_in_draw_step = (self.pool.has_triggers() || self.pool.has_replacements())
            && p == self.turn.active
            && self.turn.step == Step::Draw
            && !self.drew_in_draw_step;
        if first_in_draw_step {
            self.drew_in_draw_step = true;
        }
        if let Some(new) = self.move_object(top, ZoneId::Hand(p), true) {
            if self.pool.tracks_spells() {
                self.drawn_this_turn.push((p, new));
            }
            self.log(Event::Drew {
                player: p,
                object: new,
                first_in_draw_step,
            });
        }
    }

    pub(crate) fn lose_life(&mut self, p: PlayerId, amount: u32) {
        if amount == 0 || self.has_player_rule(p, mtg_cards::ir::PlayerRuleKind::LifeCantChange) {
            return;
        }
        self.player_mut(p).life -= i64::from(amount);
        self.log(Event::LifeLost { player: p, amount });
    }

    pub(crate) fn gain_life(&mut self, p: PlayerId, amount: u32) {
        if amount == 0
            || !self.in_game(p)
            || self.has_player_rule(p, mtg_cards::ir::PlayerRuleKind::LifeCantChange)
        {
            return;
        }
        self.player_mut(p).life += i64::from(amount);
        self.log(Event::LifeGained { player: p, amount });
    }

    pub(crate) fn tap(&mut self, id: ObjectId) {
        if let Some(o) = self.objects.get_mut(id)
            && !o.tapped
        {
            o.tapped = true;
            self.log(Event::Tapped { object: id });
        }
    }

    /// Players lose simultaneously (CR 704.3) and leave the game (CR 800.4a).
    /// The game-over check runs once afterwards, so if everyone remaining
    /// loses at once the game is a draw (CR 104.4a).
    pub(crate) fn players_lose(&mut self, losers: &[(PlayerId, LossReason)]) {
        // "You can't lose the game" stops every loss but a concession (CR 104.3a).
        let losers: SmallVec<[(PlayerId, LossReason); 8]> = losers
            .iter()
            .copied()
            .filter(|&(p, reason)| {
                self.in_game(p) && (reason == LossReason::Conceded || self.can_lose(p))
            })
            .collect();
        for &(p, reason) in &losers {
            self.player_mut(p).status = PlayerStatus::Lost(reason);
            self.log(Event::PlayerLost { player: p, reason });
        }
        for &(p, _) in &losers {
            self.remove_from_game(p);
        }
        self.check_game_over();
    }

    /// CR 104.3a: `p` concedes; they leave the game at once and lose. The
    /// pending decision is derived again for the game that remains.
    /// Returns `false` if `p` isn't in the game or the game is over.
    pub fn concede(&mut self, p: PlayerId) -> bool {
        if self.result.is_some() || !self.in_game(p) {
            return false;
        }
        self.pending = None;
        self.players_lose(&[(p, LossReason::Conceded)]);
        // The decision's task is on top of the stack and runs again against
        // the game that remains. A procedure that now needs a player who
        // left, or has no legal option, can't continue (their objects and
        // abilities left the game, CR 800.4a): it ends.
        while self.result.is_none() {
            if let Some(Task::Priority { checked }) = self.tasks.last_mut() {
                // Priority comes after state-based actions and triggers (CR 117.5).
                *checked = false;
            }
            self.run();
            match &self.pending {
                Some(d) if !self.in_game(d.player) || d.option_count() == 0 => {
                    self.pending = None;
                    self.tasks.pop();
                }
                _ => break,
            }
        }
        true
    }

    /// Whether `p` can lose the game now (not under "can't lose").
    pub(crate) fn can_lose(&self, p: PlayerId) -> bool {
        !self.player_has_rule(p, mtg_cards::ir::PlayerRuleKind::CantLose)
    }

    /// Whether `p` can win by an effect: no opponent still in the game has
    /// "your opponents can't win the game".
    pub(crate) fn can_win(&self, p: PlayerId) -> bool {
        !self.players.iter().any(|q| {
            q.id != p
                && q.in_game()
                && self.player_has_rule(q.id, mtg_cards::ir::PlayerRuleKind::OpponentsCantWin)
        })
    }

    /// CR 800.4a: everything the player owns leaves the game. (Spells they
    /// controlled but didn't own, and permanents they controlled but didn't
    /// own, need control-changing effects, which M2 doesn't have.)
    fn remove_from_game(&mut self, p: PlayerId) {
        let owned: Vec<(ObjectId, ZoneId)> = self
            .objects
            .iter()
            .filter(|(_, o)| o.owner == p)
            .map(|(id, o)| (id, o.zone))
            .collect();
        for (id, zone) in owned {
            self.detach(id, zone);
            self.commander_moved(id, None, zone);
            self.objects.free(id);
            self.drop_counters(id);
        }
        // Phased-out permanents they owned left too (CR 702.26d: still on the battlefield).
        let objects = &self.objects;
        self.phased_out.retain(|(id, _)| objects.get(*id).is_some());
        self.object_vars
            .retain(|(id, _, _)| objects.get(*id).is_some());
        self.bump_layers();
        let pl = self.player_mut(p);
        pl.library.clear();
        pl.hand.clear();
        pl.graveyard.clear();
        pl.pool.clear();
        self.restricted.retain(|r| r.player != p);
        // CR 800.4a: effects giving them control of objects end.
        self.end_effects(
            |e| matches!(e.effect, crate::layers::LayerEffect::Control { to, .. } if to == p),
        );
        // CR 800.4a/d: their abilities on the stack and waiting to go on it
        // cease to exist, as do their delayed triggers.
        self.pending_triggers.retain(|t| t.controller != p);
        self.stack.retain(|s| s.is_spell() || s.controller != p);
        self.delayed.retain(|d| d.ctx.controller != p);
        // CR 800.4a: then anything they still control is exiled.
        let controlled: Vec<ObjectId> = self
            .battlefield
            .iter()
            .chain(self.phased_out.iter().map(|(id, _)| id))
            .copied()
            .chain(self.stack.iter().filter(|s| s.is_spell()).map(|s| s.object))
            .filter(|id| self.obj(*id).is_some_and(|o| o.controller == p))
            .collect();
        for id in controlled {
            if self.phased_out.iter().any(|(x, _)| *x == id) {
                self.phased_out.retain(|(x, _)| *x != id);
                self.battlefield.push(id);
            }
            self.move_object(id, ZoneId::Exile, true);
        }
        self.log(Event::PlayerLeft { player: p });
        self.turn.passes = 0;
        // CR 800.4a: if they had priority, it passes to the next player in turn order.
        if self.turn.priority == Some(p) {
            self.turn.priority = Some(self.next_in_game(p));
        }
    }

    /// CR 104.2a / 104.4a.
    pub(crate) fn check_game_over(&mut self) {
        if self.result.is_some() {
            return;
        }
        let remaining: SmallVec<[PlayerId; 8]> = self
            .players
            .iter()
            .filter(|p| p.in_game())
            .map(|p| p.id)
            .collect();
        let turn = self.turn.number;
        match remaining.as_slice() {
            [] => self.end_game(GameResult::Draw { turn }),
            [winner] => {
                let w = *winner;
                self.player_mut(w).status = PlayerStatus::Won;
                self.end_game(GameResult::Win { player: w, turn });
            }
            _ => {}
        }
    }

    pub(crate) fn end_game(&mut self, result: GameResult) {
        self.result = Some(result);
        self.pending = None;
        self.tasks.clear();
        self.log(Event::GameOver);
    }

    // ---- the loop ----------------------------------------------------------

    /// Runs tasks until a decision is needed or the game ends.
    pub(crate) fn run(&mut self) {
        while self.pending.is_none() && self.result.is_none() {
            let Some(task) = self.tasks.pop() else {
                unreachable!("the task stack never empties while the game runs");
            };
            self.execute(task);
        }
    }

    fn execute(&mut self, task: Task) {
        match task {
            Task::DrawOpeningHands => {
                for p in self.turn_order_from(self.starting_player) {
                    for _ in 0..self.rules.starting_hand_size {
                        self.draw(p);
                    }
                }
            }
            Task::MulliganRound => {
                let order: SmallVec<[PlayerId; 8]> = self
                    .turn_order_from(self.starting_player)
                    .into_iter()
                    .filter(|p| !self.player(*p).kept)
                    .collect();
                if !order.is_empty() {
                    self.tasks.push(Task::MulliganDeclare {
                        order,
                        next: 0,
                        takers: SmallVec::new(),
                    });
                }
            }
            Task::MulliganDeclare {
                order,
                next,
                takers,
            } => self.mulligan_declare(order, next, takers),
            Task::MulliganBottom { player, remaining } => {
                let cards = self.player(player).hand.clone();
                if remaining == 0 || cards.is_empty() {
                    return;
                }
                self.tasks.push(Task::MulliganBottom { player, remaining });
                self.decide(
                    player,
                    DecisionKind::BottomCard { remaining, cards },
                    "103.5",
                );
            }
            Task::BeginTurn => self.begin_turn(),
            Task::BeginStep => self.begin_step(),
            Task::Priority { checked } => self.priority(checked),
            Task::EndStep => self.end_step(),
            Task::Resolve => self.resolve_top(),
            Task::Cast(t) => self.cast_step(t),
            Task::ManualPay(t) => self.manual_pay_step(t),
            Task::DeclareAttackers(t) => self.attack_step(t),
            Task::DeclareBlockers(t) => self.block_step(t),
            Task::CombatDamage(t) => self.damage_step(t),
            Task::Cleanup { discarded } => self.cleanup(discarded),
            Task::Sba(t) => self.sba_step(t),
            Task::Effect(t) => self.effect_step(t),
            Task::SpendMana {
                player,
                spend,
                spell,
            } => self.spend_mana(player, spend, spell),
            Task::FinishAbility { source, ability } => self.finish_ability(source, ability),
            Task::StackTriggers(t) => self.stack_triggers_step(t),
            Task::Draw(t) => self.draw_task_step(t),
            Task::Shortcut(t) => self.shortcut_step(t),
            Task::Pregame { order, next } => self.pregame_step(order, next),
            Task::FinishSpell { object } => {
                if let Some(o) = self.obj(object).copied()
                    && o.zone == ZoneId::Stack
                {
                    self.move_object(object, ZoneId::Graveyard(o.owner), true);
                }
            }
        }
    }

    fn answer(&mut self, task: Task, decision: PendingDecision, option: usize) {
        match (task, decision.kind) {
            (
                Task::MulliganDeclare {
                    order,
                    next,
                    mut takers,
                },
                DecisionKind::Mulligan { .. },
            ) => {
                let p = order[next];
                if option == 1 {
                    takers.push(p);
                } else {
                    self.player_mut(p).kept = true;
                    let hand_size = u32::try_from(self.player(p).hand.len()).unwrap_or(0);
                    self.log(Event::Kept {
                        player: p,
                        hand_size,
                    });
                }
                self.tasks.push(Task::MulliganDeclare {
                    order,
                    next: next + 1,
                    takers,
                });
            }
            (
                Task::MulliganBottom { player, remaining },
                DecisionKind::BottomCard { cards, .. },
            ) => {
                self.move_object(cards[option], ZoneId::Library(player), false);
                self.tasks.push(Task::MulliganBottom {
                    player,
                    remaining: remaining - 1,
                });
            }
            (Task::Priority { .. }, DecisionKind::Priority { actions }) => {
                self.priority_answer(decision.player, actions[option]);
            }
            (Task::Cast(t), kind) => self.cast_answer(t, kind, option),
            (Task::ManualPay(t), DecisionKind::ManaStep { steps, .. }) => {
                self.manual_pay_answer(t, steps[option]);
            }
            (
                Task::DeclareAttackers(t),
                DecisionKind::DeclareAttacker {
                    targets,
                    planeswalkers,
                    ..
                },
            ) => match targets.get(option) {
                Some(&target) => self.attack_answer(t, target.map(|p| (p, None))),
                None => {
                    let pw = planeswalkers[option - targets.len()];
                    let controller = self.obj(pw).map_or(self.turn.active, |o| o.controller);
                    self.attack_answer(t, Some((controller, Some(pw))));
                }
            },
            (Task::DeclareBlockers(t), DecisionKind::DeclareBlocker { attackers, .. }) => {
                self.block_answer(t, attackers[option]);
            }
            (Task::CombatDamage(t), DecisionKind::AssignDamage { recipients, .. }) => {
                self.damage_answer(t, recipients[option]);
            }
            (Task::Cleanup { .. }, DecisionKind::Discard { cards, .. }) => {
                let p = decision.player;
                self.move_object(cards[option], ZoneId::Graveyard(p), true);
                self.tasks.push(Task::Cleanup { discarded: false });
            }
            (Task::Sba(t), DecisionKind::LegendRule { candidates }) => {
                self.sba_answer(t, candidates[option]);
            }
            (Task::Sba(t), DecisionKind::CommanderZone { .. }) => {
                self.commander_offer_answer(option == 1);
                self.tasks.push(Task::Sba(t));
            }
            (Task::Draw(t), DecisionKind::CommanderZone { commander, .. }) => {
                self.commander_answer(commander, option == 1);
                self.tasks.push(Task::Draw(t));
            }
            (Task::Effect(t), kind) => self.effect_answer(t, kind, option),
            (Task::StackTriggers(t), DecisionKind::OrderTriggers { .. }) => {
                self.stack_triggers_answer(t, option);
            }
            (
                Task::Shortcut(t),
                kind
                @ (DecisionKind::ProposeShortcut { .. } | DecisionKind::RespondShortcut { .. }),
            ) => self.shortcut_answer(t, kind, option),
            (
                Task::Resolve,
                kind @ (DecisionKind::ChooseColor { .. } | DecisionKind::ChooseNumber { .. }),
            ) => {
                self.record_entering_choice(&kind, option);
                self.tasks.push(Task::Resolve);
            }
            (Task::Pregame { order, next }, DecisionKind::ChooseCard { cards, .. }) => {
                self.pregame_answer(order, next, cards[option]);
            }
            (Task::Draw(t), DecisionKind::ChooseReplacement { .. }) => {
                self.draw_task_answer(&t, option);
            }
            (task, kind) => unreachable!("decision {kind:?} answered for task {task:?}"),
        }
    }

    // ---- "as this enters, choose" (CR 614.12) ---------------------------------

    /// The choice `id` needs before it enters under `p`'s control, if its
    /// card has one and it hasn't been made.
    pub(crate) fn entering_question(
        &self,
        id: ObjectId,
        p: PlayerId,
    ) -> Option<(PlayerId, DecisionKind)> {
        let o = self.obj(id)?;
        let (spec, store) = self.def(o.def).as_enters()?;
        if self.object_vars.iter().any(|v| v.0 == id && v.1 == store) {
            return None;
        }
        let kind = match spec {
            ir::ChoiceSpec::Color => DecisionKind::ChooseColor {
                source: id,
                colors: mtg_core::Color::ALL.to_vec(),
            },
            ir::ChoiceSpec::Number { min, max } => DecisionKind::ChooseNumber {
                source: id,
                min: *min,
                max: *max,
            },
            ir::ChoiceSpec::CardName | ir::ChoiceSpec::Opponent => return None,
        };
        Some((p, kind))
    }

    /// Records the answer to an [`Game::entering_question`].
    pub(crate) fn record_entering_choice(&mut self, kind: &DecisionKind, option: usize) {
        let (id, value) = match kind {
            DecisionKind::ChooseColor { source, colors } => (
                *source,
                mtg_core::Color::ALL
                    .iter()
                    .position(|c| *c == colors[option])
                    .map_or(0, |i| i as i64),
            ),
            DecisionKind::ChooseNumber { source, min, .. } => {
                (*source, min + i64::try_from(option).unwrap_or(0))
            }
            _ => return,
        };
        let Some(store) = self
            .obj(id)
            .and_then(|o| self.def(o.def).as_enters())
            .map(|(_, s)| s.to_owned())
        else {
            return;
        };
        self.log(Event::Chose {
            player: self.obj(id).map_or(PlayerId(0), |o| o.controller),
            choice: format!("{store} = {value}"),
        });
        self.object_vars.push((id, store, value));
    }

    // ---- opening-hand actions (CR 103.6) -------------------------------------

    /// Cards in `p`'s hand whose opening-hand action `p` may take.
    fn pregame_candidates(&self, p: PlayerId) -> Vec<ObjectId> {
        self.player(p)
            .hand
            .iter()
            .copied()
            .filter(|id| {
                self.obj_def(*id)
                    .and_then(|d| d.opening_hand())
                    .is_some_and(|(_, o)| !o.not_starting_player || p != self.starting_player)
            })
            .collect()
    }

    fn pregame_step(&mut self, order: SmallVec<[PlayerId; 8]>, next: usize) {
        let Some(&p) = order.get(next) else {
            return;
        };
        let candidates = self.pregame_candidates(p);
        if !self.in_game(p) || candidates.is_empty() {
            self.tasks.push(Task::Pregame {
                order,
                next: next + 1,
            });
            return;
        }
        let source = candidates[0];
        let cards = std::iter::once(None)
            .chain(candidates.into_iter().map(Some))
            .collect();
        self.tasks.push(Task::Pregame { order, next });
        // In any order, one at a time; stopping ends this player's actions.
        self.decide(
            p,
            DecisionKind::ChooseCard {
                source,
                purpose: crate::decision::CardPurpose::BeginOnBattlefield,
                cards,
            },
            "103.6",
        );
    }

    fn pregame_answer(
        &mut self,
        order: SmallVec<[PlayerId; 8]>,
        next: usize,
        card: Option<ObjectId>,
    ) {
        let Some(card) = card else {
            self.tasks.push(Task::Pregame {
                order,
                next: next + 1,
            });
            return;
        };
        let p = order[next];
        self.tasks.push(Task::Pregame { order, next });
        let Some((ability, def)) = self
            .obj(card)
            .and_then(|o| Some((self.def(o.def).opening_hand()?.0, o.def)))
        else {
            return;
        };
        let ctx =
            crate::effects::EffectCtx::new(crate::effects::AbilityRef { def, ability }, card, p);
        self.tasks
            .push(Task::Effect(Box::new(crate::effects::EffectTask::new(ctx))));
    }

    // ---- mulligans (CR 103.5) ---------------------------------------------

    fn mulligan_declare(
        &mut self,
        order: SmallVec<[PlayerId; 8]>,
        next: usize,
        takers: SmallVec<[PlayerId; 8]>,
    ) {
        if let Some(&p) = order.get(next) {
            // CR 103.5: a player can mulligan until their opening hand would be zero cards.
            let can = self.rules.starting_hand_size > self.bottom_count(self.player(p).mulligans);
            if !self.in_game(p) {
                // A player who conceded makes no more declarations.
                self.tasks.push(Task::MulliganDeclare {
                    order,
                    next: next + 1,
                    takers,
                });
            } else if can {
                let mulligans_taken = self.player(p).mulligans;
                self.tasks.push(Task::MulliganDeclare {
                    order,
                    next,
                    takers,
                });
                self.decide(p, DecisionKind::Mulligan { mulligans_taken }, "103.5");
            } else {
                self.player_mut(p).kept = true;
                self.tasks.push(Task::MulliganDeclare {
                    order,
                    next: next + 1,
                    takers,
                });
            }
            return;
        }
        if takers.is_empty() {
            return;
        }
        // All mulligans happen at the same time, then the round repeats.
        self.tasks.push(Task::MulliganRound);
        let mut bottoms = Vec::new();
        for &p in &takers {
            let hand = self.player(p).hand.clone();
            for c in hand {
                self.move_object(c, ZoneId::Library(p), true);
            }
            let mut lib = std::mem::take(&mut self.player_mut(p).library);
            self.rng.shuffle(&mut lib);
            self.player_mut(p).library = lib;
            for _ in 0..self.rules.starting_hand_size {
                self.draw(p);
            }
            self.player_mut(p).mulligans += 1;
            let count = self.player(p).mulligans;
            self.log(Event::Mulligan { player: p, count });
            let k = self
                .bottom_count(count)
                .min(u32::try_from(self.player(p).hand.len()).unwrap_or(0));
            bottoms.push(Task::MulliganBottom {
                player: p,
                remaining: k,
            });
        }
        // Bottoming is sequenced in turn order (CR 101.4).
        for t in bottoms.into_iter().rev() {
            self.tasks.push(t);
        }
    }

    /// Cards to bottom after `mulligans` mulligans; the first is free in
    /// multiplayer (CR 103.5c).
    fn bottom_count(&self, mulligans: u32) -> u32 {
        if self.players.len() > 2 {
            mulligans.saturating_sub(1)
        } else {
            mulligans
        }
    }

    // ---- turns and steps ---------------------------------------------------

    fn begin_turn(&mut self) {
        let next = if self.turn.number == 0 {
            self.starting_player
        } else {
            self.next_in_game(self.turn.active)
        };
        if let Some(limit) = self.rules.turn_limit
            && self.turn.number >= limit
        {
            let turn = self.turn.number;
            self.end_game(GameResult::Aborted {
                reason: AbortReason::TurnLimit,
                turn,
            });
            return;
        }
        self.turn.number += 1;
        self.turn.active = next;
        self.turn.step = Step::Untap;
        self.turn.priority = None;
        self.turn.passes = 0;
        self.turn.extra_cleanup = false;
        let number = self.turn.number;
        self.player_mut(next).last_turn = number;
        for p in &mut self.players {
            p.lands_played = 0;
        }
        self.activated_this_turn.clear();
        self.drew_in_draw_step = false;
        self.cast_this_turn.clear();
        self.drawn_this_turn.clear();
        self.combat_damaged = 0;
        self.extra_combats = 0;
        self.log(Event::TurnBegan {
            number,
            active: next,
        });
        self.tasks.push(Task::BeginStep);
    }

    fn active_in_game(&self) -> bool {
        self.in_game(self.turn.active)
    }

    fn begin_step(&mut self) {
        // Static conditions like "during your turn" may change.
        self.bump_layers();
        let step = self.turn.step;
        self.turn.priority = None;
        self.turn.passes = 0;
        self.log(Event::StepBegan { step });
        let active = self.turn.active;
        match step {
            Step::Untap => {
                // "Until your next turn" effects end as the turn begins (CR 611.2a).
                self.player_rules.retain(|(_, _, until)| *until != active);
                // CR 502.1: phased-out permanents the active player controlled phase in first.
                self.phase_in(active);
                // CR 502.3: untap all permanents the active player controls. No priority (CR 117.3a).
                if self.active_in_game() {
                    let exempt = self.untap_exempt();
                    let ids: Vec<ObjectId> = self
                        .battlefield
                        .iter()
                        .copied()
                        .filter(|id| {
                            self.obj(*id)
                                .is_some_and(|o| o.controller == active && o.tapped)
                                && !exempt.contains(id)
                        })
                        .collect();
                    for id in ids {
                        if let Some(o) = self.objects.get_mut(id) {
                            o.tapped = false;
                        }
                        self.log(Event::Untapped { object: id });
                    }
                }
                self.tasks.push(Task::EndStep);
            }
            Step::Draw => {
                // CR 103.8a: in a two-player game the starting player skips their first draw.
                let skip = self.players.len() == 2
                    && self.turn.number == 1
                    && active == self.starting_player;
                self.tasks.push(Task::Priority { checked: false });
                if self.active_in_game() && !skip {
                    if self.pool.has_replacements() || self.draw_question(active).is_some() {
                        self.tasks.push(Task::Draw(crate::replacement::DrawTask {
                            player: active,
                            applied: SmallVec::new(),
                        }));
                    } else {
                        self.draw(active);
                    }
                }
            }
            Step::BeginCombat => {
                self.combat = Some(Combat::default());
                self.tasks.push(Task::Priority { checked: false });
            }
            Step::DeclareAttackers => {
                self.tasks.push(Task::Priority { checked: false });
                self.tasks.push(Task::DeclareAttackers(self.attack_task()));
            }
            Step::DeclareBlockers => {
                self.tasks.push(Task::Priority { checked: false });
                self.tasks.push(Task::DeclareBlockers(self.block_task()));
            }
            Step::FirstStrikeDamage | Step::CombatDamage => {
                self.tasks.push(Task::Priority { checked: false });
                let t = self.damage_task(step == Step::FirstStrikeDamage);
                self.tasks.push(Task::CombatDamage(t));
            }
            Step::Cleanup => self.tasks.push(Task::Cleanup { discarded: false }),
            Step::Upkeep
            | Step::PrecombatMain
            | Step::EndCombat
            | Step::PostcombatMain
            | Step::End => {
                self.tasks.push(Task::Priority { checked: false });
            }
        }
    }

    /// CR 500.5: as a step ends, mana pools empty; then the next step begins.
    fn end_step(&mut self) {
        for p in &mut self.players {
            p.pool.clear();
        }
        self.restricted.clear();
        let next = match self.turn.step {
            Step::Untap => Step::Upkeep,
            // CR 614.10: "skip your draw step".
            Step::Upkeep if self.skips_step(self.turn.active, Step::Draw) => Step::PrecombatMain,
            Step::Upkeep => Step::Draw,
            Step::Draw => Step::PrecombatMain,
            Step::PrecombatMain => Step::BeginCombat,
            Step::BeginCombat => Step::DeclareAttackers,
            Step::DeclareAttackers => {
                // CR 508.8: no attackers → skip declare blockers and combat damage.
                if self
                    .combat
                    .as_ref()
                    .is_some_and(|c| !c.attackers.is_empty())
                {
                    Step::DeclareBlockers
                } else {
                    Step::EndCombat
                }
            }
            Step::DeclareBlockers => {
                // CR 510.4: a first-strike damage step happens if any attacker or blocker has first or double strike.
                if self.any_first_striker() {
                    Step::FirstStrikeDamage
                } else {
                    Step::CombatDamage
                }
            }
            Step::FirstStrikeDamage => Step::CombatDamage,
            Step::CombatDamage => Step::EndCombat,
            Step::EndCombat => {
                // CR 511.3: creatures stop attacking and blocking as combat ends.
                self.combat = None;
                // CR 500.8: an additional combat phase directly after this one.
                if self.extra_combats > 0 {
                    self.extra_combats -= 1;
                    Step::BeginCombat
                } else {
                    Step::PostcombatMain
                }
            }
            Step::PostcombatMain => Step::End,
            Step::End => Step::Cleanup,
            Step::Cleanup => {
                if self.turn.extra_cleanup {
                    self.turn.extra_cleanup = false;
                    Step::Cleanup
                } else {
                    self.tasks.push(Task::BeginTurn);
                    return;
                }
            }
        };
        self.turn.step = next;
        self.tasks.push(Task::BeginStep);
    }

    /// CR 724.1: an effect ends the turn (Sundial of the Infinite). Called
    /// while that effect resolves; the rest of the turn's procedure is replaced.
    pub(crate) fn end_the_turn(&mut self) {
        // 724.1a: abilities waiting to go on the stack cease to exist.
        self.pending_triggers.clear();
        // 724.1b: exile every object on the stack, including the one resolving.
        let items = std::mem::take(&mut self.stack);
        for item in items.iter().rev() {
            if item.is_spell()
                && self
                    .obj(item.object)
                    .is_some_and(|o| o.zone == ZoneId::Stack)
            {
                self.move_object(item.object, ZoneId::Exile, true);
            }
        }
        // 724.1d: the step ends (mana empties, CR 500.5) and the game skips
        // straight to cleanup; creatures leave combat. No player gets priority.
        self.tasks.clear();
        self.combat = None;
        self.extra_combats = 0;
        for p in &mut self.players {
            p.pool.clear();
        }
        self.restricted.clear();
        self.turn.step = Step::Cleanup;
        self.tasks.push(Task::BeginStep);
        // 724.1c: state-based actions, with no triggers put on the stack.
        self.tasks.push(Task::Sba(crate::sba::SbaTask::default()));
    }

    /// CR 514: cleanup.
    fn cleanup(&mut self, discarded: bool) {
        let active = self.turn.active;
        if !discarded && self.active_in_game() {
            // CR 514.1: discard down to maximum hand size, one card at a time (D1).
            let hand = self.player(active).hand.clone();
            let excess = u32::try_from(hand.len())
                .unwrap_or(0)
                .saturating_sub(self.rules.max_hand_size);
            if excess > 0 {
                self.tasks.push(Task::Cleanup { discarded: false });
                self.decide(
                    active,
                    DecisionKind::Discard {
                        remaining: excess,
                        cards: hand,
                    },
                    "514.1",
                );
                return;
            }
        }
        // CR 514.2: damage wears off and "until end of turn" effects end.
        for id in self.battlefield.clone() {
            if let Some(o) = self.objects.get_mut(id) {
                o.damage = 0;
            }
        }
        self.end_turn_effects();
        self.prune_attachments();
        self.prune_links();
        // CR 514.3a: if SBAs would be performed, they are, then players get priority and another cleanup follows.
        // CR 514.3a: also when abilities have triggered during cleanup.
        if self.sba_pending() || !self.pending_triggers.is_empty() {
            self.turn.extra_cleanup = true;
            self.tasks.push(Task::EndStep);
            self.tasks.push(Task::Priority { checked: false });
        } else {
            self.tasks.push(Task::EndStep);
        }
    }

    // ---- priority (CR 117) -------------------------------------------------

    fn priority(&mut self, checked: bool) {
        if !checked {
            // CR 117.5: SBAs (repeatedly), then triggered abilities go on the
            // stack (CR 603.3), before anyone gets priority.
            self.tasks.push(Task::Priority { checked: true });
            if self.pool.has_triggers() {
                self.tasks.push(Task::StackTriggers(
                    crate::triggers::StackTriggersTask::new(),
                ));
            }
            self.tasks.push(Task::Sba(crate::sba::SbaTask::default()));
            return;
        }
        let holder = match self.turn.priority {
            Some(p) if self.in_game(p) => p,
            Some(p) => self.next_in_game(p),
            // CR 117.3a / 800.4j: the active player, or the next player if they left.
            None => {
                if self.active_in_game() {
                    self.turn.active
                } else {
                    self.next_in_game(self.turn.active)
                }
            }
        };
        self.turn.priority = Some(holder);
        // CR 732: a repeated state is a loop; offer or apply a shortcut first.
        if let Some(t) = self.loop_check(holder) {
            self.tasks.push(Task::Priority { checked: true });
            self.shortcut_step(t);
            return;
        }
        let actions = self.priority_actions(holder);
        self.tasks.push(Task::Priority { checked: true });
        self.clear_commander_answers();
        self.decide(holder, DecisionKind::Priority { actions }, "117.1");
    }

    /// The top card of `p`'s library if `p` may look at it and play it
    /// (Bolas's Citadel).
    pub(crate) fn top_card_visible(&self, p: PlayerId) -> Option<ObjectId> {
        if !self.pool.has_rule_statics() {
            return None;
        }
        let citadel = self.active_rule_statics().iter().any(|(_, controller, e)| {
            *controller == p
                && matches!(
                    e,
                    ir::StaticEffect::Rule(ir::RuleMod::PlayFromTopPayingLife)
                )
        });
        citadel
            .then(|| self.player(p).library.last().copied())
            .flatten()
    }

    /// Whether `p` may cast card `id` this turn by an effect's permission.
    pub(crate) fn cast_permitted(&self, id: ObjectId, p: PlayerId) -> bool {
        self.cast_permissions
            .iter()
            .any(|(c, q)| *c == id && *q == p)
    }

    /// CR 702.138a: the extra cost of casting card `id` from `p`'s graveyard
    /// with escape granted by a permanent `p` controls (Underworld Breach).
    pub(crate) fn escape_cost(&self, id: ObjectId, p: PlayerId) -> Option<Vec<ir::CostPart>> {
        let o = self.obj(id)?;
        if o.zone != ZoneId::Graveyard(p) || !self.pool.has_rule_statics() {
            return None;
        }
        self.active_rule_statics()
            .into_iter()
            .find_map(|(source, controller, e)| match e {
                ir::StaticEffect::GrantEscape {
                    affected,
                    extra_cost,
                } if controller == p && self.matches_filter(id, affected, p, Some(source)) => {
                    Some(extra_cost.clone())
                }
                _ => None,
            })
    }

    /// CR 702.61a: a spell with split second is on the stack.
    pub(crate) fn split_second_on_stack(&self) -> bool {
        self.stack.iter().any(|s| {
            s.is_spell()
                && self.obj_def(s.object).is_some_and(|d| {
                    d.abilities
                        .contains(&ir::Ability::Keyword(ir::Keyword::SplitSecond))
                })
        })
    }

    /// Every action `p` may take now (CR 117.1). Index 0 is pass. Public for
    /// benchmarks and judge tools; the pending decision already carries this list.
    pub fn priority_actions(&self, p: PlayerId) -> Vec<Action> {
        let mut actions = vec![Action::Pass];
        // CR 702.61a: under split second only mana abilities (and special
        // actions) remain.
        if self.split_second_on_stack() {
            for &id in &self.battlefield {
                if self.obj(id).is_none_or(|o| o.controller != p) {
                    continue;
                }
                for i in 0..self.mana_ability_count(id) {
                    let ability = u8::try_from(i).unwrap_or(0);
                    if self.can_activate_mana(p, id, ability) {
                        actions.push(Action::ActivateMana {
                            source: id,
                            ability,
                        });
                    }
                }
            }
            return actions;
        }
        let sorcery_timing =
            self.turn.active == p && self.turn.step.is_main() && self.stack.is_empty();
        for &id in &self.player(p).hand {
            let Some(def) = self.obj_def(id) else {
                continue;
            };
            if def.is(CardType::Land) {
                // CR 305.1, 305.2: once per turn, main phase, empty stack.
                if sorcery_timing && self.player(p).lands_played < 1 {
                    actions.push(Action::PlayLand(id));
                }
            } else if self.can_cast(p, id, sorcery_timing) {
                actions.push(Action::Cast(id));
            }
            // Abilities that work from the hand (CR 113.6b: channel, Spirit Guides).
            if def.has_hand_abilities() {
                for i in 0..self.mana_ability_count(id) {
                    let ability = u8::try_from(i).unwrap_or(0);
                    if self.can_activate_mana(p, id, ability) {
                        actions.push(Action::ActivateMana {
                            source: id,
                            ability,
                        });
                    }
                }
                for (i, (_, a)) in self.abilities_of(id).into_iter().enumerate() {
                    let ability = u8::try_from(i).unwrap_or(0);
                    if let ir::Ability::Activated(x) = a
                        && !crate::card::is_mana_ability(x)
                        && self.can_activate(p, id, ability, sorcery_timing)
                    {
                        actions.push(Action::Activate {
                            source: id,
                            ability,
                        });
                    }
                }
            }
        }
        // CR 903.8: a player may cast a commander they own from the command zone.
        if !self.cmd.is_empty() {
            for &id in &self.cmd.zone {
                if self.obj(id).is_some_and(|o| o.owner == p)
                    && self.is_commander(id)
                    && self.can_cast(p, id, sorcery_timing)
                {
                    actions.push(Action::Cast(id));
                }
            }
        }
        // Bolas's Citadel: the top card of `p`'s library, as a land or a spell.
        if let Some(top) = self.top_card_visible(p)
            && let Some(def) = self.obj_def(top)
        {
            if def.is(CardType::Land) {
                if sorcery_timing && self.player(p).lands_played < 1 {
                    actions.push(Action::PlayLand(top));
                }
            } else if self.can_cast(p, top, sorcery_timing) {
                actions.push(Action::Cast(top));
            }
        }
        // Cards in a graveyard `p` may cast: escape (Underworld Breach) or
        // a permission ("you may cast that card this turn").
        for &id in &self.player(p).graveyard {
            if self.obj_def(id).is_none_or(|d| d.is(CardType::Land)) {
                continue;
            }
            if (self.cast_permitted(id, p) || self.escape_cost(id, p).is_some())
                && self.can_cast(p, id, sorcery_timing)
            {
                actions.push(Action::Cast(id));
            }
        }
        // Cards in exile `p` may play (CR 607.2a; Opposition Agent; Ragavan;
        // Eternal Scourge).
        {
            for &id in &self.exile {
                let own_permission = self.obj(id).is_some_and(|o| o.owner == p)
                    && self
                        .obj_def(id)
                        .is_some_and(crate::card::CardDef::castable_from_exile);
                if self.exile_permission(id).is_none_or(|(q, _)| q != p)
                    && !self.cast_permitted(id, p)
                    && !own_permission
                {
                    continue;
                }
                let Some(def) = self.obj_def(id) else {
                    continue;
                };
                if def.is(CardType::Land) {
                    if sorcery_timing && self.player(p).lands_played < 1 {
                        actions.push(Action::PlayLand(id));
                    }
                } else if self.can_cast(p, id, sorcery_timing) {
                    actions.push(Action::Cast(id));
                }
            }
        }
        for &id in &self.battlefield {
            let Some(o) = self.obj(id) else { continue };
            if o.controller != p {
                continue;
            }
            for i in 0..self.mana_ability_count(id) {
                let ability = u8::try_from(i).unwrap_or(0);
                if self.can_activate_mana(p, id, ability) {
                    actions.push(Action::ActivateMana {
                        source: id,
                        ability,
                    });
                }
            }
            // CR 613: the abilities it has now; `ability` indexes that list.
            for (i, (_, a)) in self.abilities_of(id).into_iter().enumerate() {
                let ability = u8::try_from(i).unwrap_or(0);
                if let ir::Ability::Activated(x) = a
                    && !crate::card::is_mana_ability(x)
                    && self.can_activate(p, id, ability, sorcery_timing)
                {
                    actions.push(Action::Activate {
                        source: id,
                        ability,
                    });
                }
            }
        }
        actions
    }

    fn priority_answer(&mut self, p: PlayerId, action: Action) {
        match action {
            Action::Pass => {
                self.turn.passes += 1;
                if self.turn.passes as usize >= self.players_in_game() {
                    // CR 117.4: all players passed in succession.
                    self.turn.passes = 0;
                    if self.stack.is_empty() {
                        self.tasks.push(Task::EndStep);
                    } else {
                        self.tasks.push(Task::Priority { checked: false });
                        self.tasks.push(Task::Resolve);
                    }
                } else {
                    self.turn.priority = Some(self.next_in_game(p));
                    self.tasks.push(Task::Priority { checked: false });
                }
            }
            Action::PlayLand(id) => {
                // CR 116.2a: a special action; the player keeps priority (CR 117.3c).
                self.turn.passes = 0;
                self.tasks.push(Task::Priority { checked: false });
                self.player_mut(p).lands_played += 1;
                // CR 614.1a, 614.12: an "as this enters" choice replaces the
                // move (shock lands); the replacement's effect puts it there.
                if self.replace_entering(id, p, &[]) {
                    self.log(Event::LandPlayed {
                        player: p,
                        object: id,
                    });
                } else if let Some(new) = self.move_object(id, ZoneId::Battlefield, true) {
                    // The player who plays it controls it (it may be another's card).
                    self.set_controller(new, p);
                    self.log(Event::LandPlayed {
                        player: p,
                        object: new,
                    });
                }
            }
            Action::Cast(id) => {
                self.turn.passes = 0;
                self.tasks.push(Task::Priority { checked: false });
                self.begin_cast(p, id);
            }
            Action::ActivateMana { source, ability } => {
                // CR 605.3b: no stack; the player keeps priority (CR 117.3c).
                self.turn.passes = 0;
                self.tasks.push(Task::Priority { checked: false });
                self.activate_mana(source, ability, None);
            }
            Action::Activate { source, ability } => {
                self.turn.passes = 0;
                self.tasks.push(Task::Priority { checked: false });
                self.begin_activate(p, source, ability);
            }
        }
    }

    // ---- resolution (CR 608) ----------------------------------------------

    fn resolve_top(&mut self) {
        // CR 117.3b: the active player receives priority after resolution.
        self.turn.priority = None;
        let Some(item) = self.stack.last().cloned() else {
            return;
        };
        let spell_obj = self.obj(item.object).copied();
        if item.is_spell() && spell_obj.is_none() {
            self.stack.pop();
            return;
        }
        // CR 614.12: "as this enters, choose ..." is chosen as it resolves.
        if item.is_spell()
            && let Some((p, kind)) = self.entering_question(item.object, item.controller)
        {
            self.tasks.push(Task::Resolve);
            self.decide(p, kind, "614.12");
            return;
        }
        let def_id = item.source_def;
        let pool = Arc::clone(&self.pool);
        let def = pool.get(def_id);
        let slots: &[ir::TargetSpec] = match item.ability {
            Some(i) => match def.abilities.get(usize::from(i)) {
                Some(ir::Ability::Activated(x)) => &x.targets,
                Some(ir::Ability::Triggered(t)) => &t.targets,
                _ => &[],
            },
            // An Aura spell targets what it will enchant (CR 303.4a).
            None => def
                .spell()
                .map_or_else(|| def.enchant.as_slice(), |s| s.targets.as_slice()),
        };
        let slot_legal = |g: &Game, i: usize, t: Target| {
            slots
                .get(i)
                .is_some_and(|slot| g.target_still_legal(&slot.what, t, item.controller, Some(def)))
        };
        // "This ability still resolves if its target becomes illegal."
        let resolves_anyway = item.ability.is_some_and(|i| {
            matches!(
                def.abilities.get(usize::from(i)),
                Some(ir::Ability::Triggered(t)) if t.resolves_anyway
            )
        });
        // CR 608.2b: targets are rechecked; if all are illegal, it doesn't resolve.
        if !resolves_anyway
            && !item.targets.is_empty()
            && item
                .targets
                .iter()
                .zip(&item.slots)
                .all(|(t, s)| !slot_legal(self, usize::from(*s), *t))
        {
            self.log(Event::DidNotResolve {
                object: item.object,
                def: def_id,
            });
            match (item.ability, spell_obj) {
                (None, Some(o)) => {
                    self.move_object(item.object, ZoneId::Graveyard(o.owner), true);
                }
                (Some(ability), _) => self.finish_ability(item.object, ability),
                (None, None) => {}
            }
            return;
        }
        self.log(Event::Resolved {
            object: item.object,
            def: def_id,
        });
        let ability = match item.ability {
            Some(i) => {
                // CR 608.2n: the ability is removed from the stack last.
                self.tasks.push(Task::FinishAbility {
                    source: item.object,
                    ability: i,
                });
                Some(i)
            }
            None => {
                if def.is_permanent_card() {
                    // CR 614.1a: "if this would enter, … instead" (Mox Diamond).
                    if self.replace_entering(item.object, item.controller, &[]) {
                        return;
                    }
                    // CR 608.3a: a permanent spell becomes a permanent under its controller's control.
                    // CR 614.1c: "enters with N counters" (X is the spell's X).
                    let with_counters = def.abilities.iter().find_map(|a| match a {
                        ir::Ability::Replacement(r) => match (&r.event, &r.instead) {
                            (
                                ir::ReplaceEvent::WouldEnter(f),
                                ir::Effect::EntersWithCounters { counter, amount },
                            ) if f.this => Some((*counter, amount.clone())),
                            _ => None,
                        },
                        _ => None,
                    });
                    let (controller, x) = (item.controller, item.x);
                    if let Some(new) =
                        self.move_object_with(item.object, ZoneId::Battlefield, true, |g, new| {
                            if let Some(o) = g.objects.get_mut(new) {
                                o.controller = controller;
                            }
                            g.bump_layers();
                            if let Some((kind, amount)) = &with_counters {
                                let mut ctx = EffectCtx::new(
                                    AbilityRef {
                                        def: def_id,
                                        ability: 0,
                                    },
                                    new,
                                    controller,
                                );
                                ctx.x = x;
                                let n = g.amount(&ctx, amount);
                                g.add_counters(new, *kind, n);
                            }
                        })
                    {
                        // CR 303.4f: an Aura spell enters attached to its target.
                        if def.enchant.is_some()
                            && let Some(&host) = item.targets.first()
                        {
                            self.attach(new, host, false);
                        }
                        // CR 702.109a: dashed, it has haste and returns to its
                        // owner's hand at the beginning of the next end step.
                        if item.cost == crate::decision::CastCost::Dash {
                            self.add_effect(
                                SmallVec::from_elem(new, 1),
                                crate::layers::LayerEffect::GrantKeyword(Box::new(
                                    ir::Keyword::Haste,
                                )),
                                crate::layers::Duration::Permanent,
                            );
                            self.delayed.push(crate::triggers::DelayedTrigger {
                                trigger: ir::Trigger::BeginningOf {
                                    step: ir::StepName::End,
                                    whose: ir::PlayerRel::Any,
                                },
                                path: SmallVec::new(),
                                ctx: EffectCtx::new(
                                    AbilityRef {
                                        def: def_id,
                                        ability: crate::effects::DASH_RETURN,
                                    },
                                    new,
                                    controller,
                                ),
                            });
                        }
                    }
                    return;
                }
                // CR 608.2n: an instant or sorcery goes to its owner's graveyard last.
                self.tasks.push(Task::FinishSpell {
                    object: item.object,
                });
                // Overloaded (CR 702.96): its alternative cost's text instead.
                match item.cost {
                    crate::decision::CastCost::Alternative(i)
                        if crate::effects::alt_effect(def, i).is_some() =>
                    {
                        Some(crate::effects::ALT_EFFECT_BASE + i)
                    }
                    _ => def
                        .abilities
                        .iter()
                        .position(|a| matches!(a, ir::Ability::Spell(_)))
                        .and_then(|i| u8::try_from(i).ok()),
                }
            }
        };
        if let Some(ability) = ability {
            let mut ctx = EffectCtx::new(
                AbilityRef {
                    def: def_id,
                    ability,
                },
                item.object,
                item.controller,
            );
            // CR 608.2b: illegal targets aren't affected.
            for (t, s) in item.targets.iter().zip(&item.slots) {
                let i = usize::from(*s);
                if ctx.targets.len() <= i {
                    ctx.targets.resize(i + 1, SmallVec::new());
                }
                if slot_legal(self, i, *t) {
                    ctx.targets[i].push(*t);
                }
            }
            ctx.modes.clone_from(&item.modes);
            ctx.x = item.x;
            if let Some(d) = &item.delayed {
                // CR 603.7: a delayed trigger runs its effect in its creator's context.
                let mut dctx = d.ctx.clone();
                if let Some(info) = item.trigger {
                    dctx.triggering_object = info.object;
                    dctx.triggering_player = info.player;
                    dctx.triggering_mana = info.mana;
                    dctx.triggering_amount = info.amount;
                }
                self.tasks.push(Task::Effect(Box::new(EffectTask {
                    ctx: dctx,
                    frames: vec![crate::effects::Frame::Run(d.path.clone())],
                })));
                return;
            }
            if let Some(info) = item.trigger {
                ctx.triggering_object = info.object;
                ctx.triggering_player = info.player;
                ctx.triggering_mana = info.mana;
                ctx.triggering_ability = info.stack_ability;
                ctx.triggering_amount = info.amount;
                ctx.lki.extend(info.lki);
                // CR 603.4: an intervening "if" is checked again on resolution.
                if let Some(ir::Ability::Triggered(t)) = def.abilities.get(usize::from(ability))
                    && let Some(c) = &t.condition
                    && !self.condition(&ctx, c)
                {
                    return;
                }
            }
            self.tasks
                .push(Task::Effect(Box::new(EffectTask::new(ctx))));
        }
    }

    /// CR 608.2n: an ability leaves the stack and ceases to exist.
    pub(crate) fn finish_ability(&mut self, source: ObjectId, ability: u8) {
        if let Some(pos) = self
            .stack
            .iter()
            .rposition(|s| s.object == source && s.ability == Some(ability))
        {
            self.stack.remove(pos);
        }
        self.prune_links();
    }

    /// CR 607.2a: a permanent's links end once it has left the battlefield
    /// and none of its abilities is waiting or on the stack (Worldgorger
    /// Dragon's leaves trigger still returns what its enters trigger exiled).
    pub(crate) fn prune_links(&mut self) {
        if self.linked.is_empty() {
            return;
        }
        let linked = std::mem::take(&mut self.linked);
        self.linked = linked
            .into_iter()
            .filter(|(s, _, _)| {
                self.obj(*s).is_some_and(|o| o.zone == ZoneId::Battlefield)
                    || self.stack.iter().any(|i| i.object == *s)
                    || self.pending_triggers.iter().any(|p| p.source == *s)
            })
            .collect();
    }

    /// Deals damage (CR 120.3). Lifelink and deathtouch come from the source's keywords.
    pub(crate) fn deal_damage(
        &mut self,
        source: ObjectId,
        target: Target,
        amount: u32,
        combat: bool,
    ) {
        if amount == 0 {
            return;
        }
        let kw = self.keywords(source);
        match target {
            Target::StackAbility { .. } => {}
            Target::Player(p) => {
                if !self.in_game(p) {
                    return;
                }
                // CR 702.16e, 702.16j: protection from everything prevents it.
                if self.has_player_rule(p, mtg_cards::ir::PlayerRuleKind::ProtectionFromEverything)
                {
                    self.log(Event::DamagePrevented {
                        source,
                        target,
                        amount,
                    });
                    return;
                }
                if combat && self.pool.has_triggers() {
                    self.combat_damaged |= 1 << p.0;
                }
                // Angel's Grace: damage can't take this player below 1 life.
                let amount = if self
                    .has_player_rule(p, mtg_cards::ir::PlayerRuleKind::DamageCantReduceBelowOne)
                {
                    let life = self.player(p).life;
                    u32::try_from((life - 1).max(0)).unwrap_or(0).min(amount)
                } else {
                    amount
                };
                // CR 903.10a: combat damage from a commander is tallied.
                if combat && !self.cmd.is_empty() {
                    self.record_commander_damage(source, p, amount);
                }
                self.log(Event::Damage {
                    source,
                    target,
                    amount,
                    combat,
                });
                self.lose_life(p, amount);
            }
            Target::Object(id) => {
                // CR 120.3c: damage to a planeswalker removes loyalty counters.
                if self.obj(id).is_some_and(|o| o.zone == ZoneId::Battlefield)
                    && !self.is_creature(id)
                    && self.chars(id).is_some_and(|c| c.is(CardType::Planeswalker))
                {
                    self.log(Event::Damage {
                        source,
                        target,
                        amount,
                        combat,
                    });
                    self.remove_counters(id, ir::CounterKind::Loyalty, amount);
                    return;
                }
                // CR 702.16e: damage from a source with the quality is prevented.
                if let Some(src) = self.obj(source).copied()
                    && self.protected_from(id, self.def(src.def), src.controller)
                {
                    self.log(Event::DamagePrevented {
                        source,
                        target,
                        amount,
                    });
                    return;
                }
                let Some(o) = self.objects.get_mut(id) else {
                    return;
                };
                if o.zone != ZoneId::Battlefield {
                    return;
                }
                o.damage += amount;
                if kw.has(Keywords::DEATHTOUCH) {
                    o.deathtouch_damage = true;
                }
                self.log(Event::Damage {
                    source,
                    target,
                    amount,
                    combat,
                });
            }
        }
        if kw.has(Keywords::LIFELINK) {
            // CR 702.15b
            if let Some(c) = self.obj(source).map(|o| o.controller) {
                self.gain_life(c, amount);
            }
        }
    }
}
