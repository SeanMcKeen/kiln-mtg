//! The decision contract (`docs/03 Architecture/Decision & Action Model.md`).
//!
//! The engine runs until a player must choose, then exposes one
//! [`PendingDecision`] whose options are the complete legal set. Every
//! decision is "pick one of N" (M2 plan, D1): a [`Choice`] names the decision
//! and an option index. Where a "decline" option exists, it is index 0 (D2).

use mtg_core::{CrRef, ObjectId, PlayerId};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::card::ManaType;
use crate::state::Target;

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
}

/// Mana sources to activate and the pool mana to spend for one way of paying
/// a cost (CR 601.2g–h). Plans are de-duplicated by outcome (M2 plan, D6).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaymentPlan {
    pub activations: SmallVec<[(ObjectId, u8); 8]>,
    /// Mana spent from the pool after the activations, by [`ManaType`] index.
    pub spend: [u32; 6],
}

/// One step of manual payment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ManaStep {
    /// Activate a mana ability (CR 605.3a); the mana goes to the pool.
    Activate { source: ObjectId, ability: u8 },
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
    /// CR 601.2c
    ChooseTarget {
        spell: ObjectId,
        targets: Vec<Target>,
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
}

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
            DecisionKind::DeclareAttacker { targets, .. } => targets.len(),
            DecisionKind::DeclareBlocker { attackers, .. } => attackers.len(),
            DecisionKind::AssignDamage { recipients, .. } => recipients.len(),
            DecisionKind::LegendRule { candidates } => candidates.len(),
        }
    }

    /// Whether option 0 is a "decline" option (pass, keep, no attack, no block).
    pub fn has_decline(&self) -> bool {
        match &self.kind {
            DecisionKind::Mulligan { .. }
            | DecisionKind::Priority { .. }
            | DecisionKind::DeclareAttacker { .. } => true,
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
