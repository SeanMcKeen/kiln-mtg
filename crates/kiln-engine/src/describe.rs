//! Events in words, each with its rule (the judge tool and live play, M10;
//! [[Judge Tool UX]]: "every event shows its CR rule").

use std::collections::BTreeMap;

use kiln_types::{CrRef, ObjectId};

use crate::engine::Game;
use crate::events::Event;
use crate::state::{Target, ZoneId};

/// One logged event: its index in [`Game::events`], its rule and a sentence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DescribedEvent {
    pub index: usize,
    pub cr: CrRef,
    pub text: String,
}

pub(crate) fn zone(z: ZoneId) -> String {
    match z {
        ZoneId::Library(p) => format!("{p}'s library"),
        ZoneId::Hand(p) => format!("{p}'s hand"),
        ZoneId::Graveyard(p) => format!("{p}'s graveyard"),
        ZoneId::Battlefield => "the battlefield".into(),
        ZoneId::Stack => "the stack".into(),
        ZoneId::Exile => "exile".into(),
        ZoneId::Command => "the command zone".into(),
    }
}

impl Game {
    /// Every logged event, described with card names (objects keep the name
    /// they had, even after leaving play). Needs `GameRules::log_events`.
    pub fn describe_events(&self) -> Vec<DescribedEvent> {
        let mut names: BTreeMap<ObjectId, String> = BTreeMap::new();
        let mut out = Vec::with_capacity(self.events.len());
        for (index, e) in self.events.iter().enumerate() {
            if let Event::ZoneChange { def, old, new, .. } = e {
                let n = self.def(*def).name.clone();
                names.insert(*old, n.clone());
                names.insert(*new, n);
            }
            if let Event::TokenCreated { object, def, .. } | Event::SpellCast { object, def, .. } =
                e
            {
                names.insert(*object, self.def(*def).name.clone());
            }
            let name = |id: &ObjectId| {
                names
                    .get(id)
                    .cloned()
                    .or_else(|| self.object_name(*id).map(str::to_owned))
                    .unwrap_or_else(|| format!("{id}"))
            };
            let target = |t: &Target| match t {
                Target::Player(p) => p.to_string(),
                Target::Object(o) => name(o),
                Target::StackAbility { source, .. } => format!("an ability of {}", name(source)),
            };
            let ability = |def: &crate::card::DefId, i: &u8| {
                format!("{} ability {}", self.def(*def).name, i + 1)
            };
            let text = match e {
                Event::GameStarted { starting_player } => {
                    format!("the game starts; {starting_player} goes first")
                }
                Event::Mulligan { player, count } => format!("{player} takes mulligan {count}"),
                Event::Kept { player, hand_size } => format!("{player} keeps {hand_size} cards"),
                Event::TurnBegan { number, active } => format!("turn {number} ({active}) begins"),
                Event::StepBegan { step } => format!("{step:?} step begins"),
                Event::Drew { player, .. } => format!("{player} draws a card"),
                Event::ZoneChange {
                    def,
                    from,
                    to,
                    controller,
                    ..
                } => format!(
                    "{} moves from {} to {} ({controller})",
                    self.def(*def).name,
                    zone(*from),
                    zone(*to)
                ),
                Event::LandPlayed { player, object } => format!("{player} plays {}", name(object)),
                Event::SpellCast { player, def, .. } => {
                    format!("{player} casts {}", self.def(*def).name)
                }
                Event::Triggered {
                    def,
                    ability: a,
                    controller,
                    ..
                } => {
                    format!("{} triggers ({controller})", ability(def, a))
                }
                Event::AbilityActivated {
                    player,
                    def,
                    ability: a,
                    ..
                } => {
                    format!("{player} activates {}", ability(def, a))
                }
                Event::Resolved { def, .. } => format!("{} resolves", self.def(*def).name),
                Event::DidNotResolve { def, .. } => {
                    format!(
                        "{} doesn't resolve (its targets are illegal)",
                        self.def(*def).name
                    )
                }
                Event::BecameTarget {
                    target: t, source, ..
                } => {
                    format!("{} becomes the target of {}", name(t), name(source))
                }
                Event::Damage {
                    source,
                    target: t,
                    amount,
                    combat,
                } => format!(
                    "{} deals {amount}{} damage to {}",
                    name(source),
                    if *combat { " combat" } else { "" },
                    target(t)
                ),
                Event::DamagePrevented {
                    source,
                    target: t,
                    amount,
                } => {
                    format!(
                        "{amount} damage from {} to {} is prevented",
                        name(source),
                        target(t)
                    )
                }
                Event::LifeGained { player, amount } => format!("{player} gains {amount} life"),
                Event::LifeLost { player, amount } => format!("{player} loses {amount} life"),
                Event::Tapped { object } => format!("{} taps", name(object)),
                Event::Untapped { object } => format!("{} untaps", name(object)),
                Event::AttackerDeclared { object, defending } => {
                    format!("{} attacks {defending}", name(object))
                }
                Event::EnteredAttacking { object, defending } => {
                    format!("{} enters attacking {defending}", name(object))
                }
                Event::BlockerDeclared { blocker, attacker } => {
                    format!("{} blocks {}", name(blocker), name(attacker))
                }
                Event::Destroyed { object } => format!("{} is destroyed", name(object)),
                Event::Sacrificed { object } => format!("{} is sacrificed", name(object)),
                Event::Countered { object } => format!("{} is countered", name(object)),
                Event::Discarded { player, object } => {
                    format!("{player} discards {}", name(object))
                }
                Event::Revealed { player, objects } => format!(
                    "{player} reveals {}",
                    objects.iter().map(&name).collect::<Vec<_>>().join(", ")
                ),
                Event::Shuffled { player } => format!("{player} shuffles their library"),
                Event::Searched { player, found } => {
                    format!("{player} searches their library and finds {found}")
                }
                Event::Scried { player, count } => format!("{player} scries {count}"),
                Event::Chose { player, choice } => format!("{player} chooses {choice}"),
                Event::CountersPut {
                    object,
                    counter,
                    amount,
                } => {
                    format!("{amount} {counter:?} counter(s) put on {}", name(object))
                }
                Event::CountersRemoved {
                    object,
                    counter,
                    amount,
                } => {
                    format!(
                        "{amount} {counter:?} counter(s) removed from {}",
                        name(object)
                    )
                }
                Event::TappedForMana { player, source, .. } => {
                    format!("{player} taps {} for mana", name(source))
                }
                Event::ManaAdded { player, mana, .. } => format!("{player} adds {mana:?}"),
                Event::TokenCreated { player, def, .. } => {
                    format!("{player} creates a {} token", self.def(*def).name)
                }
                Event::Attached { object, to } => {
                    format!("{} becomes attached to {}", name(object), target(to))
                }
                Event::CopyCreated { object, of } => {
                    format!("{} is created as a copy of {}", name(object), name(of))
                }
                Event::Replaced {
                    def, ability: a, ..
                } => format!("{} replaces an event", ability(def, a)),
                Event::TriggerRemoved {
                    def, ability: a, ..
                } => {
                    format!("{} is removed from the stack", ability(def, a))
                }
                Event::AbilityCopied {
                    source,
                    ability: a,
                    controller,
                } => {
                    format!("{controller} copies ability {} of {}", a + 1, name(source))
                }
                Event::Voted { player, choice } => format!("{player} votes for {choice}"),
                Event::LookedAtHand { viewer, player } => {
                    format!("{viewer} looks at {player}'s hand")
                }
                Event::ShortcutTaken { iterations, .. } => {
                    format!("a loop is shortcut: {iterations} iterations")
                }
                Event::LoopDraw => "the game is a draw (an unbreakable loop)".into(),
                Event::WonByEffect { player } => format!("{player} wins the game by an effect"),
                Event::PlayerLost { player, reason } => {
                    format!("{player} loses the game ({reason:?})")
                }
                Event::PlayerLeft { player } => format!("{player} leaves the game"),
                Event::GameOver => "the game is over".into(),
            };
            out.push(DescribedEvent {
                index,
                cr: e.cr(),
                text,
            });
        }
        out
    }

    /// Why event `index` happened: the latest cast, activation, trigger or
    /// resolution before it (the cause, in the judge tool's `why`).
    pub fn cause_of(&self, index: usize) -> Option<usize> {
        self.events.get(..index)?.iter().rposition(|e| {
            matches!(
                e,
                Event::SpellCast { .. }
                    | Event::AbilityActivated { .. }
                    | Event::Triggered { .. }
                    | Event::Resolved { .. }
                    | Event::AttackerDeclared { .. }
                    | Event::BlockerDeclared { .. }
                    | Event::Replaced { .. }
            )
        })
    }
}
