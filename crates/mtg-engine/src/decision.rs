//! The decision contract (`docs/03 Architecture/Decision & Action Model.md`).
//!
//! The engine runs until a player must choose, then exposes one
//! [`PendingDecision`] whose options are the complete legal set. Every
//! decision is "pick one of N" (M2 plan, D1): a [`Choice`] names the decision
//! and an option index. Where a "decline" option exists, it is index 0 (D2).

use mtg_core::{Color, CrRef, ManaSymbol, ObjectId, PlayerId};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::card::{DefId, ManaType};
use crate::state::{Target, ZoneId};

/// Monotonic id of a decision within a game (R7). A choice must echo it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DecisionId(pub u64);

/// A player's answer: an option of the named decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Choice {
    pub decision: DecisionId,
    pub option: u32,
}

/// An action available while holding priority (CR 117.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    /// CR 117.3d
    Pass,
    /// Special action (CR 116.2a, 305.1).
    PlayLand(ObjectId),
    /// CR 601
    Cast(ObjectId),
    /// Activate a mana ability (CR 117.1d, 605.3a). `ability` indexes the definition's mana abilities.
    ActivateMana { source: ObjectId, ability: u8 },
    /// Activate a non-mana activated ability (CR 117.1b, 602). `ability`
    /// indexes the definition's abilities.
    Activate { source: ObjectId, ability: u8 },
}

/// Mana sources to activate and the pool mana to spend for one way of paying
/// a cost (CR 601.2g–h). Plans are de-duplicated by outcome (M2 plan, D6).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaymentPlan {
    /// `(source, mana ability index, the mana it makes)`.
    pub activations: SmallVec<[(ObjectId, u8, ManaBag); 8]>,
    /// Mana spent from the pool after the activations, by [`ManaType`] index.
    pub spend: [u32; 6],
}

/// One step of manual payment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ManaStep {
    /// Activate a mana ability (CR 605.3a) making `bag`; the mana goes to the pool.
    Activate {
        source: ObjectId,
        ability: u8,
        bag: ManaBag,
    },
    /// Spend one mana of this type from the pool toward the cost.
    Spend(ManaType),
}

/// Who a creature assigns a point of combat damage to (CR 510.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DamageRecipient {
    Creature(ObjectId),
    Player(PlayerId),
}

/// What is being decided, with the legal options.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DecisionKind {
    /// CR 103.5: option 0 keeps, option 1 takes a mulligan.
    Mulligan { mulligans_taken: u32 },
    /// CR 103.5: put one card from hand on the bottom of the library (`remaining` more to go, including this one).
    BottomCard {
        remaining: u32,
        cards: Vec<ObjectId>,
    },
    /// CR 117: `actions[0]` is always [`Action::Pass`].
    Priority { actions: Vec<Action> },
    /// CR 601.2c: the next target for target slot `slot`. `targets[0]` is
    /// `None` ("no more targets") when the slot allows fewer ("up to N").
    ChooseTarget {
        spell: ObjectId,
        slot: u8,
        targets: Vec<Option<Target>>,
    },
    /// CR 601.2g–h: one option per plan, then a final "pay manually" option.
    PayMana {
        spell: ObjectId,
        plans: Vec<PaymentPlan>,
    },
    /// CR 601.2g–h, manual mode.
    ManaStep {
        spell: ObjectId,
        steps: Vec<ManaStep>,
    },
    /// CR 508.1a: `targets[0]` is `None` (doesn't attack).
    DeclareAttacker {
        creature: ObjectId,
        targets: Vec<Option<PlayerId>>,
        /// Planeswalkers it may attack (CR 508.1b); options after `targets`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        planeswalkers: Vec<ObjectId>,
    },
    /// CR 509.1a: `attackers[0]` is `None` (doesn't block).
    DeclareBlocker {
        creature: ObjectId,
        attackers: Vec<Option<ObjectId>>,
    },
    /// CR 510.1c, 702.19b: assign the next point of `source`'s combat damage.
    AssignDamage {
        source: ObjectId,
        remaining: u32,
        recipients: Vec<DamageRecipient>,
    },
    /// CR 514.1: discard one card (`remaining` more to go, including this one).
    Discard {
        remaining: u32,
        cards: Vec<ObjectId>,
    },
    /// CR 704.5j: choose the legendary permanent to keep.
    LegendRule { candidates: Vec<ObjectId> },
    /// CR 903.9a–b: the owner chooses whether a commander goes to the
    /// command zone. `to` is the zone it's in (903.9a) or would be put into
    /// (903.9b). Option 0 leaves it there; option 1 puts it into the command zone.
    CommanderZone { commander: ObjectId, to: ZoneId },
    /// A yes/no choice while an effect resolves (CR 608.2d): option 0 declines, option 1 accepts.
    Optional {
        source: ObjectId,
        kind: OptionalKind,
    },
    /// Pick the next card for `purpose`. `cards[0]` is `None` (stop) when
    /// stopping is allowed; ordering decisions pick the next card to place.
    ChooseCard {
        source: ObjectId,
        purpose: CardPurpose,
        cards: Vec<Option<ObjectId>>,
    },
    /// Choose which mana an effect adds (CR 106.1b): one option per outcome.
    ChooseMana {
        source: ObjectId,
        options: Vec<ManaBag>,
    },
    /// Choose a card name (CR 201.4): the card names in the game's pool (never
    /// a token's), plus `None` for "a name not in this game", which is
    /// outcome-equivalent to every other absent name (M3 plan, B-D5).
    ChooseName {
        source: ObjectId,
        names: Vec<Option<DefId>>,
    },
    /// Choose a number from `min` to `max` inclusive (option i = `min + i`).
    ChooseNumber {
        source: ObjectId,
        min: i64,
        max: i64,
    },
    /// Choose a player (an opponent, for "an opponent gains control").
    ChoosePlayer {
        source: ObjectId,
        players: Vec<PlayerId>,
    },
    /// What a creature put onto the battlefield attacking attacks: a
    /// defending player, or a planeswalker after them (CR 508.4).
    ChooseAttacked {
        creature: ObjectId,
        players: Vec<PlayerId>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        planeswalkers: Vec<ObjectId>,
    },
    /// Vote for one of `choices` (CR 701.38a); with `extra`, option 0
    /// declines an additional vote (CR 701.38d).
    Vote {
        source: ObjectId,
        choices: Vec<String>,
        extra: bool,
    },
    /// Choose a color (CR 105.1).
    ChooseColor {
        source: ObjectId,
        colors: Vec<Color>,
    },
    /// CR 601.2b, 700.2: the next mode of a modal spell; `modes[0]` is
    /// `None` (done) once enough modes are chosen.
    ChooseMode {
        spell: ObjectId,
        modes: Vec<Option<u8>>,
    },
    /// CR 603.3b: the next of this player's triggered abilities to put on the
    /// stack (`(source, definition, ability index)`; the last one put on the
    /// stack resolves first).
    OrderTriggers {
        triggers: Vec<(ObjectId, DefId, u8)>,
    },
    /// CR 732.2a: repeat a detected loop. Option 0 plays on; option `i`
    /// repeats it `counts[i - 1]` times.
    ProposeShortcut { counts: Vec<u64> },
    /// CR 732.2b: accept a shortcut (`proposed` iterations; `None` for an
    /// endless mandatory loop) with option 0, or shorten it to
    /// `stops[i - 1]` iterations and act there.
    RespondShortcut {
        proposed: Option<u64>,
        stops: Vec<u64>,
    },
    /// CR 707.10c: keep a copied spell's target (option 0) or change it to
    /// `options[i - 1]`.
    ChangeTarget {
        spell: ObjectId,
        index: u8,
        current: Target,
        options: Vec<Target>,
    },
    /// CR 616.1: which replacement effect to apply to the event first
    /// (`(source, definition, ability index)`).
    ChooseReplacement { options: Vec<(ObjectId, DefId, u8)> },
    /// CR 601.2b, 118.9: pay the mana cost or an alternative cost.
    ChooseCost {
        spell: ObjectId,
        options: Vec<CastCost>,
    },
    /// CR 601.2b, 107.4e–f: how to pay one hybrid or Phyrexian symbol.
    ChooseSymbolPay {
        spell: ObjectId,
        symbol: ManaSymbol,
        options: Vec<SymbolPay>,
    },
}

/// Which cost a spell is cast for (CR 118.9a: at most one alternative cost).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum CastCost {
    /// The mana cost.
    #[default]
    Normal,
    /// The spell's alternative cost with this index.
    Alternative(u8),
    /// "Without paying its mana cost" (CR 118.9), when an effect casts it.
    WithoutPaying,
    /// The dash cost (CR 702.109a).
    Dash,
    /// The escape cost from the graveyard (CR 702.138a).
    Escape,
    /// Life equal to its mana value rather than its mana cost (Bolas's Citadel).
    LifeInsteadOfMana,
}

/// How one hybrid or Phyrexian mana symbol is paid (CR 107.4e–f).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolPay {
    /// One mana of this type.
    Mana(ManaType),
    /// Two generic mana (monocolored hybrid `{2/W}`).
    TwoGeneric,
    /// 2 life (Phyrexian).
    TwoLife,
}

/// What a yes/no choice is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OptionalKind {
    /// "You may …" (CR 608.2d).
    May,
    /// "[Player] may [action]. If they do, …": whether to take the action.
    IfDoes,
    /// "… unless [player] pays [cost]" (CR 118.12): whether to pay.
    PayUnless,
    /// Whether to pay a spell's kicker (CR 702.33a).
    Kicker,
    /// Whether to repeat the process ("any number of times").
    Repeat,
    /// "Tap or untap": option 0 taps, option 1 untaps (Pestermite).
    TapOrUntap,
}

/// Why a card is being chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CardPurpose {
    /// Find a card while searching (CR 701.23).
    Search,
    /// Discard it (CR 701.9a).
    Discard,
    /// Sacrifice it (CR 701.21a).
    Sacrifice,
    /// Exile it.
    Exile,
    /// Put it on top of its owner's library next (the last one placed ends up on top).
    PutOnTop,
    /// Put it on the bottom of its owner's library next (CR 701.22a for scry).
    PutOnBottom,
    /// Take it from the looked-at cards.
    Take,
    /// Return it to its owner's hand.
    ReturnToHand,
    /// Put it onto the battlefield.
    PutOntoBattlefield,
    /// Put it into its owner's graveyard.
    PutIntoGraveyard,
    /// Put counters on it (amass, CR 701.47a).
    PutCounters,
    /// Enter as a copy of it (CR 707.9, 614.1c).
    Copy,
    /// Take its opening-hand action: begin the game with it on the battlefield (CR 103.6).
    BeginOnBattlefield,
}

pub use crate::card::ManaBag;

/// A decision awaiting a [`Choice`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PendingDecision {
    pub id: DecisionId,
    pub player: PlayerId,
    pub kind: DecisionKind,
    /// The rule that creates the decision (R3).
    pub cr: CrRef,
}

impl PendingDecision {
    pub fn option_count(&self) -> usize {
        match &self.kind {
            DecisionKind::Mulligan { .. } => 2,
            DecisionKind::BottomCard { cards, .. } | DecisionKind::Discard { cards, .. } => {
                cards.len()
            }
            DecisionKind::Priority { actions } => actions.len(),
            DecisionKind::ChooseTarget { targets, .. } => targets.len(),
            DecisionKind::PayMana { plans, .. } => plans.len() + 1,
            DecisionKind::ManaStep { steps, .. } => steps.len(),
            DecisionKind::DeclareAttacker {
                targets,
                planeswalkers,
                ..
            } => targets.len() + planeswalkers.len(),
            DecisionKind::DeclareBlocker { attackers, .. } => attackers.len(),
            DecisionKind::AssignDamage { recipients, .. } => recipients.len(),
            DecisionKind::LegendRule { candidates } => candidates.len(),
            DecisionKind::Optional { .. } | DecisionKind::CommanderZone { .. } => 2,
            DecisionKind::ChooseCard { cards, .. } => cards.len(),
            DecisionKind::ChooseMana { options, .. } => options.len(),
            DecisionKind::ChooseName { names, .. } => names.len(),
            DecisionKind::ChooseNumber { min, max, .. } => {
                usize::try_from(max - min + 1).unwrap_or(0)
            }
            DecisionKind::ChooseColor { colors, .. } => colors.len(),
            DecisionKind::ChoosePlayer { players, .. } => players.len(),
            DecisionKind::ChooseAttacked {
                players,
                planeswalkers,
                ..
            } => players.len() + planeswalkers.len(),
            DecisionKind::Vote { choices, extra, .. } => choices.len() + usize::from(*extra),
            DecisionKind::ChooseMode { modes, .. } => modes.len(),
            DecisionKind::OrderTriggers { triggers } => triggers.len(),
            DecisionKind::ChooseReplacement { options } => options.len(),
            DecisionKind::ChangeTarget { options, .. } => options.len() + 1,
            DecisionKind::ProposeShortcut { counts } => counts.len() + 1,
            DecisionKind::RespondShortcut { stops, .. } => stops.len() + 1,
            DecisionKind::ChooseCost { options, .. } => options.len(),
            DecisionKind::ChooseSymbolPay { options, .. } => options.len(),
        }
    }

    /// Whether option 0 is a "decline" option (pass, keep, no attack, no block).
    pub fn has_decline(&self) -> bool {
        match &self.kind {
            DecisionKind::Mulligan { .. }
            | DecisionKind::Priority { .. }
            | DecisionKind::DeclareAttacker { .. }
            | DecisionKind::Optional { .. }
            | DecisionKind::CommanderZone { .. } => true,
            DecisionKind::ChooseCard { cards, .. } => cards.first().is_some_and(Option::is_none),
            DecisionKind::ChooseTarget { targets, .. } => {
                targets.first().is_some_and(Option::is_none)
            }
            DecisionKind::ChooseMode { modes, .. } => modes.first().is_some_and(Option::is_none),
            DecisionKind::Vote { extra, .. } => *extra,
            // Not blocking can be illegal when it would strand a menace attacker.
            DecisionKind::DeclareBlocker { attackers, .. } => {
                attackers.first().is_some_and(Option::is_none)
            }
            _ => false,
        }
    }
}

/// Why a choice was rejected. Rejection is a client bug, never a way to probe rules.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChoiceError {
    #[error("no decision is pending (the game is over)")]
    NoDecision,
    #[error("choice answers decision {got:?} but decision {expected:?} is pending")]
    WrongDecision {
        expected: DecisionId,
        got: DecisionId,
    },
    #[error("option {option} is out of range; decision {decision:?} has {count} options")]
    OutOfRange {
        decision: DecisionId,
        option: u32,
        count: usize,
    },
}
