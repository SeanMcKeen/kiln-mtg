//! The greedy baseline (`docs/14 Players & Policies/A - Baseline Policies.md`,
//! M3 plan B-D12).
//!
//! One-ply heuristics over the [`PlayerView`] only: play a land, cast the
//! most expensive useful spell, aim harmful effects at opponents and
//! helpful ones at itself, attack and block when its creature survives.
//! It's a yardstick for the statistics in M7, not a strong player.

use mtg_core::{CardType, ObjectId, PlayerId};
use mtg_engine::decision::{CardPurpose, OptionalKind};
use mtg_engine::ir::{self, Effect, TargetKind};
use mtg_engine::{Action, CardDef, DecisionKind, PendingDecision, PlayerView, Target};

use crate::Policy;

/// The greedy baseline policy. Deterministic: ties go to the lowest option index.
#[derive(Debug, Clone, Copy, Default)]
pub struct GreedyPolicy;

/// Whether an effect hurts what it touches (damage, removal, countering…).
fn harmful(e: &Effect) -> bool {
    let mut bad = false;
    e.walk(&mut |x| {
        bad |= match x {
            Effect::DealDamage { .. }
            | Effect::Destroy(_)
            | Effect::Exile { .. }
            | Effect::Counter(_)
            | Effect::Tap(_)
            | Effect::Move { .. }
            | Effect::Sacrifice { .. }
            | Effect::Unless { .. } => true,
            Effect::Discard { who, .. } | Effect::LoseLife { who, .. } => {
                !matches!(who, ir::PlayerRef::You)
            }
            Effect::PutCounters { counter, .. } => *counter == ir::CounterKind::MinusOne,
            Effect::Pump { power, .. } => match power {
                ir::Quantity::Fixed(n) => *n < 0,
                ir::Quantity::Expr(ir::QuantityExpr::Neg(_)) => true,
                ir::Quantity::Expr(_) => false,
            },
            _ => false,
        };
    });
    bad
}

/// The effect of a spell, or of activated ability `ability` of a definition.
fn effect_of(def: &CardDef, ability: Option<u8>) -> Option<&Effect> {
    match ability {
        Some(i) => match def.abilities.get(usize::from(i))? {
            ir::Ability::Activated(x) => Some(&x.effect),
            _ => None,
        },
        None => def.spell().map(|s| &s.effect),
    }
}

impl GreedyPolicy {
    fn lands_in_play(view: &PlayerView<'_>) -> usize {
        view.battlefield()
            .iter()
            .filter(|id| {
                view.object(**id).is_some_and(|o| o.controller == view.me())
                    && view.card(**id).is_some_and(|d| d.is(CardType::Land))
            })
            .count()
    }

    fn lands_in_hand(view: &PlayerView<'_>) -> usize {
        view.hand()
            .iter()
            .filter(|id| view.card(**id).is_some_and(|d| d.is(CardType::Land)))
            .count()
    }

    /// How much keeping a card is worth (for discards, bottoms and sacrifices).
    fn keep_value(view: &PlayerView<'_>, id: ObjectId) -> i64 {
        let Some(d) = view.card(id) else { return 0 };
        if d.token {
            return -10;
        }
        if d.is(CardType::Land) {
            return if Self::lands_in_play(view) + Self::lands_in_hand(view) < 5 {
                40
            } else {
                5
            };
        }
        let power = view.object(id).and_then(|o| o.power).unwrap_or(0);
        i64::from(d.mana_value()) * 10 + i64::from(power)
    }

    /// Whether an opponent has a permanent of one of the filter's card types
    /// (a cheap stand-in for "this removal spell has a good target").
    fn opponent_has(view: &PlayerView<'_>, kind: &TargetKind) -> bool {
        let types: &[String] = match kind {
            TargetKind::Object(f) => &f.types,
            _ => return true,
        };
        view.battlefield().iter().any(|id| {
            view.object(*id).is_some_and(|o| o.controller != view.me())
                && view.card(*id).is_some_and(|d| {
                    types.is_empty()
                        || types.iter().any(|t| {
                            CardType::ALL
                                .iter()
                                .any(|ct| ct.word().eq_ignore_ascii_case(t) && d.is(*ct))
                        })
                })
        })
    }

    fn cast_score(view: &PlayerView<'_>, id: ObjectId) -> i64 {
        let Some(d) = view.card(id) else { return 0 };
        let stack = view.stack();
        if let Some(spell) = d.spell() {
            let targets_spells = spell
                .targets
                .iter()
                .any(|t| matches!(t.what, TargetKind::Spell(_)));
            if targets_spells {
                // Counter only an opponent's spell.
                return if stack.last().is_some_and(|s| s.controller != view.me()) {
                    500
                } else {
                    0
                };
            }
            if !stack.is_empty() {
                return 0;
            }
            if harmful(&spell.effect)
                && !spell
                    .targets
                    .iter()
                    .all(|t| Self::opponent_has(view, &t.what))
            {
                return 0;
            }
        } else if !stack.is_empty() {
            return 0;
        }
        100 + i64::from(d.mana_value()) * 10
    }

    fn activate_score(view: &PlayerView<'_>, source: ObjectId, ability: u8) -> i64 {
        if !view.stack().is_empty() {
            return 0;
        }
        let Some(ir::Ability::Activated(x)) = view.object_ability(source, ability) else {
            return 0;
        };
        if harmful(&x.effect) && !x.targets.iter().all(|t| Self::opponent_has(view, &t.what)) {
            return 0;
        }
        50
    }

    fn priority(view: &PlayerView<'_>, actions: &[Action]) -> usize {
        let mut best = (0, 0i64);
        for (i, a) in actions.iter().enumerate() {
            let score = match *a {
                Action::PlayLand(_) => 1000,
                Action::Cast(id) => Self::cast_score(view, id),
                Action::Activate { source, ability } => Self::activate_score(view, source, ability),
                Action::Pass | Action::ActivateMana { .. } => 0,
            };
            if score > best.1 {
                best = (i, score);
            }
        }
        best.0
    }

    fn target_score(view: &PlayerView<'_>, t: Option<Target>, bad: bool) -> i64 {
        let me = view.me();
        let mine = |p: PlayerId| p == me;
        match t {
            None => 0,
            Some(Target::Player(p)) => match (bad, mine(p)) {
                (true, true) | (false, false) => -100,
                (true, false) => 20 + view.player(p).life,
                (false, true) => 100,
            },
            Some(Target::Object(id)) => {
                let controller = view
                    .object(id)
                    .map(|o| o.controller)
                    .or_else(|| {
                        view.stack()
                            .iter()
                            .find(|s| s.object == id)
                            .map(|s| s.controller)
                    })
                    .unwrap_or(me);
                let value = Self::keep_value(view, id).max(1);
                match (bad, mine(controller)) {
                    (true, true) | (false, false) => -100,
                    (true, false) | (false, true) => 30 + value,
                }
            }
        }
    }

    fn choose_target(view: &PlayerView<'_>, spell: ObjectId, targets: &[Option<Target>]) -> usize {
        let entry = view.stack().into_iter().rev().find(|s| s.object == spell);
        let bad = entry
            .and_then(|s| effect_of(view.definition(s.def), s.ability).map(harmful))
            .unwrap_or(true);
        argmax(targets.iter().map(|t| Self::target_score(view, *t, bad)))
    }

    fn attack(view: &PlayerView<'_>, creature: ObjectId, targets: &[Option<PlayerId>]) -> usize {
        let Some(me) = view.object(creature) else {
            return 0;
        };
        let (power, toughness) = (me.power.unwrap_or(0), me.toughness.unwrap_or(0));
        if power <= 0 {
            return 0;
        }
        let mut best = (0, i64::MIN);
        for (i, t) in targets.iter().enumerate() {
            let Some(p) = t else { continue };
            let blockers = view.untapped_creatures(*p);
            let safe = blockers.iter().all(|b| {
                view.object(*b)
                    .and_then(|o| o.power)
                    .is_none_or(|bp| bp < toughness)
            });
            // Prefer the opponent with the lowest life among safe attacks.
            let score = -view.player(*p).life;
            if safe && score > best.1 {
                best = (i, score);
            }
        }
        best.0
    }

    fn block(view: &PlayerView<'_>, creature: ObjectId, attackers: &[Option<ObjectId>]) -> usize {
        let Some(me) = view.object(creature) else {
            return 0;
        };
        let (power, toughness) = (me.power.unwrap_or(0), me.toughness.unwrap_or(0));
        let incoming: i64 = attackers
            .iter()
            .flatten()
            .filter_map(|a| view.object(*a).and_then(|o| o.power))
            .map(i64::from)
            .sum();
        let in_danger = incoming >= view.player(view.me()).life;
        let mut best = (usize::MAX, i64::MIN);
        for (i, a) in attackers.iter().enumerate() {
            let Some(a) = a else { continue };
            let Some(o) = view.object(*a) else { continue };
            let (ap, at) = (o.power.unwrap_or(0), o.toughness.unwrap_or(0));
            let survives = ap < toughness;
            let kills = power >= at;
            let score = match (survives, kills) {
                (true, true) => 300 + i64::from(ap),
                (true, false) => 100 + i64::from(ap),
                (false, true) => 50 + i64::from(ap),
                (false, false) if in_danger => i64::from(ap),
                (false, false) => continue,
            };
            if score > best.1 {
                best = (i, score);
            }
        }
        if best.0 == usize::MAX {
            // Not blocking is option 0 when it's legal; otherwise block somewhere.
            0
        } else {
            best.0
        }
    }

    fn choose_card(
        view: &PlayerView<'_>,
        purpose: CardPurpose,
        cards: &[Option<ObjectId>],
    ) -> usize {
        let lands = Self::lands_in_play(view);
        match purpose {
            CardPurpose::Search | CardPurpose::Take => argmax(cards.iter().map(|c| match c {
                None => -1,
                Some(id) => {
                    let land = view.card(*id).is_some_and(|d| d.is(CardType::Land));
                    if land && lands < 4 {
                        1000
                    } else {
                        Self::keep_value(view, *id)
                    }
                }
            })),
            CardPurpose::PutOnBottom => argmax(cards.iter().map(|c| match c {
                None => 0,
                Some(id) => {
                    let land = view.card(*id).is_some_and(|d| d.is(CardType::Land));
                    if land && lands >= 5 { 1 } else { -1 }
                }
            })),
            CardPurpose::Discard | CardPurpose::Sacrifice | CardPurpose::Exile => {
                argmax(cards.iter().map(|c| match c {
                    None => i64::MIN,
                    Some(id) => -Self::keep_value(view, *id),
                }))
            }
            // Ordering and moving: the first card.
            _ => cards.iter().position(Option::is_some).unwrap_or(0),
        }
    }

    fn mulligan(view: &PlayerView<'_>, taken: u32) -> usize {
        let lands = Self::lands_in_hand(view);
        usize::from(taken < 2 && !(2..=5).contains(&lands))
    }
}

/// Index of the highest score; ties go to the lowest index.
fn argmax(scores: impl Iterator<Item = i64>) -> usize {
    let mut best = (0, i64::MIN);
    for (i, s) in scores.enumerate() {
        if s > best.1 {
            best = (i, s);
        }
    }
    best.0
}

impl Policy for GreedyPolicy {
    fn name(&self) -> String {
        "greedy".into()
    }

    fn decide(&mut self, view: &PlayerView<'_>, d: &PendingDecision) -> u32 {
        let i = match &d.kind {
            DecisionKind::Mulligan { mulligans_taken } => Self::mulligan(view, *mulligans_taken),
            DecisionKind::BottomCard { cards, .. } | DecisionKind::Discard { cards, .. } => {
                argmax(cards.iter().map(|c| -Self::keep_value(view, *c)))
            }
            DecisionKind::Priority { actions } => Self::priority(view, actions),
            DecisionKind::ChooseTarget { spell, targets, .. } => {
                Self::choose_target(view, *spell, targets)
            }
            DecisionKind::DeclareAttacker { creature, targets } => {
                Self::attack(view, *creature, targets)
            }
            DecisionKind::DeclareBlocker {
                creature,
                attackers,
            } => Self::block(view, *creature, attackers),
            DecisionKind::ChooseCard { purpose, cards, .. } => {
                Self::choose_card(view, *purpose, cards)
            }
            // "You may", "if you do", and paying to stop an "unless" effect.
            DecisionKind::Optional { kind, .. } => match kind {
                OptionalKind::May | OptionalKind::IfDoes | OptionalKind::PayUnless => 1,
            },
            // As much X as can be paid.
            DecisionKind::ChooseNumber { min, max, .. } => usize::try_from(max - min).unwrap_or(0),
            DecisionKind::ChooseMode { modes, .. } => {
                modes.iter().position(Option::is_some).unwrap_or(0)
            }
            // Payment plan 0, the first legal option elsewhere.
            DecisionKind::PayMana { .. }
            | DecisionKind::ManaStep { .. }
            | DecisionKind::AssignDamage { .. }
            | DecisionKind::LegendRule { .. }
            | DecisionKind::ChooseMana { .. }
            | DecisionKind::ChooseName { .. }
            | DecisionKind::ChooseColor { .. }
            | DecisionKind::ChooseCost { .. }
            | DecisionKind::ChooseSymbolPay { .. }
            | DecisionKind::OrderTriggers { .. }
            | DecisionKind::ChooseReplacement { .. }
            | DecisionKind::ChangeTarget { .. }
            // Accept shortcuts others propose.
            | DecisionKind::RespondShortcut { .. } => 0,
            // Take the longest shortcut on offer.
            DecisionKind::ProposeShortcut { counts } => counts.len(),
        };
        u32::try_from(i.min(d.option_count().saturating_sub(1))).unwrap_or(0)
    }
}
