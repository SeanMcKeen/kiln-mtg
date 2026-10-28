//! Player views (`docs/03 Architecture/Hidden Information & Player Views.md`)
//! and human-readable option labels.
//!
//! Policies see only a [`PlayerView`]: public zones, their own hand, and
//! counts of hidden zones. Hidden objects' ids are never exposed, because
//! ids could otherwise correlate with card identities (Design Principle 5).

use kiln_cards::ir::CounterKind;
use kiln_types::{ObjectId, PlayerId};
use serde::Serialize;

use crate::card::{CardDef, DefId, ManaBag, ManaType};
use crate::decision::{
    Action, CardPurpose, CastCost, DamageRecipient, DecisionKind, ManaStep, OptionalKind,
    PendingDecision, SymbolPay,
};
use crate::engine::Game;
use crate::state::{Step, Target, TurnState, ZoneId};

/// A borrowed, redacted view of a game for one player.
#[derive(Debug, Clone, Copy)]
pub struct PlayerView<'g> {
    pub(crate) game: &'g Game,
    me: PlayerId,
}

/// An object the viewer can see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VisibleObject {
    pub id: ObjectId,
    pub def: DefId,
    pub name: String,
    pub owner: PlayerId,
    pub controller: PlayerId,
    pub zone: ZoneId,
    pub tapped: bool,
    pub damage: u32,
    /// Effective on the battlefield; printed elsewhere.
    pub power: Option<i32>,
    pub toughness: Option<i32>,
    pub attacking: Option<PlayerId>,
    pub blocking: Option<ObjectId>,
    /// CR 122.
    pub counters: Vec<(CounterKind, u32)>,
    /// CR 111.
    pub token: bool,
    /// A commander (CR 903.3; public information).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub commander: bool,
}

/// A spell or ability on the stack (public, CR 400.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StackEntry {
    /// The spell, or the ability's source.
    pub object: ObjectId,
    pub def: DefId,
    pub name: String,
    pub controller: PlayerId,
    /// `Some(i)`: activated ability `i` of the source.
    pub ability: Option<u8>,
    pub targets: Vec<Target>,
}

/// Public facts about a player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PublicPlayer {
    pub id: PlayerId,
    pub in_game: bool,
    pub life: i64,
    pub poison: u32,
    pub hand_count: usize,
    pub library_count: usize,
    pub mana_pool: [u32; 6],
}

/// A serializable snapshot of a view (for LLMs, MCP, logs and the redaction auditor).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ViewSnapshot {
    pub me: PlayerId,
    pub turn: u32,
    pub active: PlayerId,
    pub step: Step,
    pub players: Vec<PublicPlayer>,
    pub hand: Vec<VisibleObject>,
    pub battlefield: Vec<VisibleObject>,
    /// Bottom first.
    pub stack: Vec<StackEntry>,
    pub graveyards: Vec<Vec<VisibleObject>>,
    pub exile: Vec<VisibleObject>,
    /// The command zone (CR 408; public, CR 400.2).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<VisibleObject>,
    /// The top card of the viewer's library, when they may look at it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library_top: Option<VisibleObject>,
    /// The hand of the player the viewer controls this turn (CR 723.4).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub controlled_hand: Vec<VisibleObject>,
}

/// "Target Rhystic Study ability 1", with "[k]" telling identical ones apart.
fn stack_ability_label(verb: &str, source: &str, ability: u8, nth: u8) -> String {
    let mut s = format!("{verb} {source} ability {}", ability + 1);
    if nth > 0 {
        s.push_str(&format!(" [{}]", nth + 1));
    }
    s
}

/// Mana as symbols, e.g. `{C}{C}`.
fn bag_text(bag: &ManaBag) -> String {
    let mut s = String::new();
    for t in ManaType::ALL {
        for _ in 0..bag[t.index()] {
            s.push_str(t.symbol());
        }
    }
    s
}

impl Game {
    /// The view for `me`.
    pub fn view(&self, me: PlayerId) -> PlayerView<'_> {
        PlayerView { game: self, me }
    }

    /// Invariant 5: the view snapshot for `me` exposes no hidden object
    /// (another player's hand, any library).
    pub fn audit_view(&self, me: PlayerId) -> Result<(), String> {
        let snap = self.view(me).snapshot();
        let mut exposed: Vec<ObjectId> = snap
            .hand
            .iter()
            .chain(&snap.battlefield)
            .chain(&snap.exile)
            .chain(&snap.command)
            .chain(&snap.library_top)
            .chain(&snap.controlled_hand)
            .map(|o| o.id)
            .collect();
        // An ability's source may have left play (CR 113.7a); only spells are stack objects.
        exposed.extend(
            snap.stack
                .iter()
                .filter(|e| e.ability.is_none())
                .map(|e| e.object),
        );
        exposed.extend(snap.graveyards.iter().flatten().map(|o| o.id));
        for id in exposed {
            let Some(o) = self.obj(id) else {
                return Err(format!("view exposes dead object {id}"));
            };
            match o.zone {
                ZoneId::Library(p)
                    if !(self.sees_as(me, p) && self.top_card_visible(p) == Some(id)) =>
                {
                    return Err(format!("view for {me} exposes library object {id}"));
                }
                ZoneId::Hand(p) if !self.sees_as(me, p) => {
                    return Err(format!("view for {me} exposes {p}'s hand object {id}"));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn name_of(&self, id: ObjectId) -> String {
        // Its current name (a copy has the copied name, CR 707.2).
        self.object_name(id)
            .map_or_else(|| format!("{id}"), str::to_owned)
    }

    /// A short label for option `i` of the pending decision, used by scenario
    /// scripts and human/LLM interfaces. Labels name cards, not ids.
    pub fn option_label(&self, i: usize) -> Option<String> {
        let d = self.pending.as_ref()?;
        Some(self.label(d, i))
    }

    fn label(&self, d: &PendingDecision, i: usize) -> String {
        let name = |id: ObjectId| self.name_of(id);
        let who = |id: ObjectId| {
            let n = name(id);
            match self.obj(id) {
                Some(o) if o.zone == ZoneId::Battlefield => format!("{n} ({})", o.controller),
                _ => n,
            }
        };
        match &d.kind {
            DecisionKind::Mulligan { .. } => if i == 0 { "Keep" } else { "Mulligan" }.to_owned(),
            DecisionKind::BottomCard { cards, .. } => format!("Bottom {}", name(cards[i])),
            DecisionKind::Discard { cards, .. } => format!("Discard {}", name(cards[i])),
            DecisionKind::Priority { actions } => match actions[i] {
                Action::Pass => "Pass".into(),
                Action::PlayLand(id) => format!("Play {}", name(id)),
                Action::Cast(id) => format!("Cast {}", name(id)),
                Action::ActivateMana { source, ability } => {
                    let list = self.mana_abilities_of(source);
                    let m = list.get(ability as usize).copied();
                    let verb = if m.is_some_and(|(_, m)| m.tap_only) {
                        "Tap"
                    } else {
                        "Activate"
                    };
                    let controller = self.obj(source).map_or(d.player, |o| o.controller);
                    let bags = m.map(|(r, m)| self.mana_ability_bags(r, m, source, controller));
                    let what = match bags.as_deref() {
                        Some([bag]) => bag_text(bag),
                        _ => "mana".to_owned(),
                    };
                    format!("{verb} {} for {what}", name(source))
                }
                // Abilities are numbered from 1 in printed order (labels avoid YAML-hostile brackets).
                Action::Activate { source, ability } => {
                    format!("Activate {} ability {}", name(source), ability + 1)
                }
            },
            DecisionKind::ChooseTarget { targets, .. } => match targets[i] {
                None => "No more targets".into(),
                Some(Target::Player(p)) => format!("Target {p}"),
                Some(Target::Object(id)) => format!("Target {}", who(id)),
                Some(Target::StackAbility {
                    source,
                    ability,
                    nth,
                }) => stack_ability_label("Target", &name(source), ability, nth),
            },
            DecisionKind::PayMana { plans, .. } => match plans.get(i) {
                None => "Pay manually".into(),
                Some(plan) => {
                    let mut parts: Vec<String> = plan
                        .activations
                        .iter()
                        .map(|(id, _, _)| name(*id))
                        .collect();
                    let produced: u32 = plan
                        .activations
                        .iter()
                        .map(|(_, _, bag)| bag.iter().map(|n| u32::from(*n)).sum::<u32>())
                        .sum();
                    let pool_used = plan.spend.iter().sum::<u32>().saturating_sub(produced);
                    if pool_used > 0 {
                        parts.push(format!("{pool_used} from pool"));
                    }
                    format!("Pay with {}", parts.join(", "))
                }
            },
            DecisionKind::ManaStep { steps, .. } => match steps[i] {
                ManaStep::Activate { source, bag, .. } => {
                    format!("Tap {} for {}", name(source), bag_text(&bag))
                }
                ManaStep::Spend(m) => format!("Spend {m}"),
            },
            DecisionKind::DeclareAttacker {
                creature,
                targets,
                planeswalkers,
            } => match targets.get(i) {
                Some(None) => format!("{} doesn't attack", name(*creature)),
                Some(Some(p)) => format!("{} attacks {p}", name(*creature)),
                None => format!(
                    "{} attacks {}",
                    name(*creature),
                    who(planeswalkers[i - targets.len()])
                ),
            },
            DecisionKind::DeclareBlocker {
                creature,
                attackers,
            } => match attackers[i] {
                None => format!("{} doesn't block", name(*creature)),
                Some(a) => format!("{} blocks {}", name(*creature), who(a)),
            },
            DecisionKind::AssignDamage { recipients, .. } => match recipients[i] {
                DamageRecipient::Creature(id) => format!("1 damage to {}", who(id)),
                DamageRecipient::Player(p) => format!("1 damage to {p}"),
            },
            // Same-name permanents are told apart by position (1-based, battlefield order).
            DecisionKind::LegendRule { candidates } => {
                format!("Keep {} [{}]", name(candidates[i]), i + 1)
            }
            DecisionKind::CommanderZone { commander, to } => {
                let zone = match to {
                    ZoneId::Hand(_) => "hand",
                    ZoneId::Library(_) => "library",
                    ZoneId::Graveyard(_) => "graveyard",
                    _ => "exile",
                };
                let already = self.obj(*commander).is_some_and(|o| o.zone == *to);
                match (i, already) {
                    (0, true) => format!("Leave {} in {zone}", name(*commander)),
                    (0, false) => format!("Put {} into {zone}", name(*commander)),
                    _ => format!("Put {} into the command zone", name(*commander)),
                }
            }
            DecisionKind::Optional { kind, .. } => match (kind, i) {
                (OptionalKind::May | OptionalKind::IfDoes, 0) => "No".into(),
                (OptionalKind::May | OptionalKind::IfDoes, _) => "Yes".into(),
                (OptionalKind::PayUnless, 0) => "Don't pay".into(),
                (OptionalKind::PayUnless, _) => "Pay".into(),
                (OptionalKind::Kicker, 0) => "Don't kick".into(),
                (OptionalKind::Kicker, _) => "Kick".into(),
                (OptionalKind::Repeat, 0) => "Stop".into(),
                (OptionalKind::Repeat, _) => "Repeat".into(),
                (OptionalKind::TapOrUntap, 0) => "Tap".into(),
                (OptionalKind::TapOrUntap, _) => "Untap".into(),
            },
            DecisionKind::ChooseCard { purpose, cards, .. } => match cards[i] {
                None => "Stop".into(),
                Some(id) => {
                    let verb = match purpose {
                        CardPurpose::Search => "Find",
                        CardPurpose::Discard => "Discard",
                        CardPurpose::Sacrifice => "Sacrifice",
                        CardPurpose::Tap => "Tap",
                        CardPurpose::Exile => "Exile",
                        CardPurpose::PutOnTop => "Put on top",
                        CardPurpose::PutOnBottom => "Put on bottom",
                        CardPurpose::Take => "Take",
                        CardPurpose::ReturnToHand => "Return to hand",
                        CardPurpose::PutOntoBattlefield => "Put onto the battlefield",
                        CardPurpose::PutIntoGraveyard => "Put into graveyard",
                        CardPurpose::PutCounters => "Put counters on",
                        CardPurpose::Copy => "Copy",
                        CardPurpose::Enchant => "Enchant",
                        CardPurpose::BeginOnBattlefield => "Begin the game with",
                    };
                    format!("{verb} {}", who(id))
                }
            },
            DecisionKind::ChoosePlayer { players, .. } => format!("Choose {}", players[i]),
            DecisionKind::ChooseAttacked {
                creature,
                players,
                planeswalkers,
            } => match players.get(i) {
                Some(p) => format!("{} attacks {p}", name(*creature)),
                None => format!(
                    "{} attacks {}",
                    name(*creature),
                    who(planeswalkers[i - players.len()])
                ),
            },
            DecisionKind::Vote { choices, extra, .. } => match (extra, i) {
                (true, 0) => "Don't vote again".into(),
                (true, _) => format!("Vote {}", choices[i - 1]),
                (false, _) => format!("Vote {}", choices[i]),
            },
            DecisionKind::ChooseMana { options, .. } => {
                let mut s = String::from("Add ");
                for t in ManaType::ALL {
                    for _ in 0..options[i][t.index()] {
                        s.push_str(t.symbol());
                    }
                }
                s
            }
            DecisionKind::ChooseName { names, .. } => match names[i] {
                Some(d) => format!("Name {}", self.def(d).name),
                None => "Name a card not in this game".into(),
            },
            DecisionKind::ChooseNumber { min, .. } => {
                format!("Choose {}", min + i64::try_from(i).unwrap_or(0))
            }
            DecisionKind::ChooseColor { colors, .. } => format!("Choose {:?}", colors[i]),
            DecisionKind::OrderTriggers { triggers } => {
                let (_, def, ability) = triggers[i];
                format!(
                    "Put {} ability {} on the stack",
                    self.def(def).name,
                    ability + 1
                )
            }
            DecisionKind::ProposeShortcut { counts } => match i {
                0 => "Don't take a shortcut".into(),
                _ => format!("Repeat the loop {} times", counts[i - 1]),
            },
            DecisionKind::RespondShortcut { proposed, stops } => match (i, proposed) {
                (0, Some(n)) => format!("Accept {n} iterations"),
                (0, None) => "Let the loop continue".into(),
                _ if stops[i - 1] == 0 => "Act before the next iteration".into(),
                _ => format!("Stop after {} iterations", stops[i - 1]),
            },
            DecisionKind::ChangeTarget {
                current, options, ..
            } => match i {
                0 => match current {
                    Target::Player(p) => format!("Keep target {p}"),
                    Target::Object(id) => format!("Keep target {}", who(*id)),
                    Target::StackAbility {
                        source,
                        ability,
                        nth,
                    } => stack_ability_label("Keep target", &name(*source), *ability, *nth),
                },
                _ => match options[i - 1] {
                    Target::Player(p) => format!("New target {p}"),
                    Target::Object(id) => format!("New target {}", who(id)),
                    Target::StackAbility {
                        source,
                        ability,
                        nth,
                    } => stack_ability_label("New target", &name(source), ability, nth),
                },
            },
            DecisionKind::ChooseReplacement { options } => {
                let (_, def, ability) = options[i];
                format!("Apply {} ability {}", self.def(def).name, ability + 1)
            }
            DecisionKind::ChooseMode { modes, .. } => match modes[i] {
                None => "No more modes".into(),
                Some(m) => format!("Mode {}", m + 1),
            },
            DecisionKind::ChooseCost { options, .. } => match options[i] {
                CastCost::Normal => "Pay the mana cost".into(),
                CastCost::Alternative(a) => format!("Pay alternative cost {}", a + 1),
                CastCost::WithoutPaying => "Cast without paying its mana cost".into(),
                CastCost::Dash => "Pay the dash cost".into(),
                CastCost::Escape => "Pay the escape cost".into(),
                CastCost::LifeInsteadOfMana => "Pay life equal to its mana value".into(),
            },
            DecisionKind::ChooseSymbolPay {
                symbol, options, ..
            } => match options[i] {
                SymbolPay::Mana(t) => format!("Pay {symbol} with {t}"),
                SymbolPay::TwoGeneric => format!("Pay {symbol} with {{2}}"),
                SymbolPay::TwoLife => format!("Pay {symbol} with 2 life"),
            },
        }
    }
}

impl<'g> PlayerView<'g> {
    pub fn me(&self) -> PlayerId {
        self.me
    }

    pub fn turn(&self) -> &'g TurnState {
        &self.game.turn
    }

    pub fn player_count(&self) -> usize {
        self.game.players.len()
    }

    /// The pending decision, if this viewer makes it (their own, or the
    /// player they control's, CR 723.5).
    pub fn pending(&self) -> Option<&'g PendingDecision> {
        self.game
            .pending
            .as_ref()
            .filter(|d| self.game.decider_for(d.player) == self.me)
    }

    pub fn option_label(&self, i: usize) -> Option<String> {
        self.pending().and_then(|_| self.game.option_label(i))
    }

    pub fn player(&self, p: PlayerId) -> PublicPlayer {
        let pl = self.game.player(p);
        PublicPlayer {
            id: p,
            in_game: pl.in_game(),
            life: pl.life,
            poison: pl.poison,
            hand_count: pl.hand.len(),
            library_count: pl.library.len(),
            mana_pool: pl.pool.0,
        }
    }

    /// Whether the viewer may see object `id` (CR 400.2).
    pub fn can_see(&self, id: ObjectId) -> bool {
        self.game.obj(id).is_some_and(|o| match o.zone {
            // Bolas's Citadel: its controller may look at their top card.
            ZoneId::Library(p) => {
                self.game.sees_as(self.me, p) && self.game.top_card_visible(p) == Some(id)
            }
            // CR 723.4: also the hand of the player `me` controls.
            ZoneId::Hand(p) => self.game.sees_as(self.me, p),
            // CR 406.3
            ZoneId::Exile => !self.game.face_down.contains(&id),
            _ => true,
        })
    }

    /// The definition of a visible object.
    pub fn card(&self, id: ObjectId) -> Option<&'g CardDef> {
        if self.can_see(id) {
            self.game.obj_def(id)
        } else {
            None
        }
    }

    /// Ability `i` of a visible object as it is now (CR 613): the index an
    /// `Action::Activate` carries.
    pub fn object_ability(&self, id: ObjectId, i: u8) -> Option<&'g kiln_cards::ir::Ability> {
        if !self.can_see(id) {
            return None;
        }
        self.game
            .abilities_of(id)
            .get(usize::from(i))
            .map(|(_, a)| *a)
    }

    pub fn object(&self, id: ObjectId) -> Option<VisibleObject> {
        if !self.can_see(id) {
            return None;
        }
        let o = self.game.obj(id)?;
        // What it is now (a copy shows what it copies, CR 707.2).
        let def_id = self.game.chars(id).map_or(o.def, |c| c.def_id());
        let def = self.game.def(def_id);
        let combat = self.game.combat.as_ref();
        Some(VisibleObject {
            id,
            def: def_id,
            name: def.name.clone(),
            owner: o.owner,
            controller: o.controller,
            zone: o.zone,
            tapped: o.tapped,
            damage: o.damage,
            power: def.power.map(|p| {
                if o.zone == ZoneId::Battlefield {
                    self.game.power(id)
                } else {
                    p
                }
            }),
            toughness: def.toughness.map(|t| {
                if o.zone == ZoneId::Battlefield {
                    self.game.toughness(id)
                } else {
                    t
                }
            }),
            attacking: combat.and_then(|c| c.attacker(id)).map(|a| a.defending),
            blocking: combat.and_then(|c| c.blocks.iter().find(|(b, _)| *b == id).map(|(_, a)| *a)),
            counters: self.game.counters_of(id),
            token: def.token,
            commander: self.game.is_commander(id),
        })
    }

    /// The logged events as this viewer may know them, from index `from` on
    /// (a card moving between hidden zones is "a card"). Empty unless the
    /// game logs events.
    pub fn events_since(&self, from: usize) -> Vec<crate::describe::DescribedEvent> {
        let mut v = self.game.describe_events_for(Some(self.me));
        v.drain(..from.min(v.len()));
        v
    }

    /// How many events the game has logged.
    pub fn event_count(&self) -> usize {
        self.game.events.len()
    }

    /// The exile zone (public, CR 406.3; face-down cards hide themselves
    /// through [`PlayerView::object`]).
    pub fn exile(&self) -> &'g [ObjectId] {
        &self.game.exile
    }

    /// How the game ended, if it has (public).
    pub fn result(&self) -> Option<&'g crate::engine::GameResult> {
        self.game.result()
    }

    /// An owned copy of the game behind this view, for sandboxes that need
    /// a `'static` handle (playbooks). Read it only through
    /// [`Game::view`] for the same viewer, so nothing hidden leaks.
    pub fn game_copy(&self) -> Game {
        self.game.clone()
    }

    /// A player's graveyard (public, CR 404.2).
    pub fn graveyard(&self, p: PlayerId) -> &'g [ObjectId] {
        &self.game.player(p).graveyard
    }

    /// The command zone (public, CR 408).
    pub fn command(&self) -> &'g [ObjectId] {
        &self.game.cmd.zone
    }

    pub fn hand(&self) -> &'g [ObjectId] {
        &self.game.player(self.me).hand
    }

    /// What visible object `id` is attached to (public, CR 701.3).
    pub fn attached_to(&self, id: ObjectId) -> Option<Target> {
        if !self.can_see(id) {
            return None;
        }
        self.game.attached_to(id)
    }

    pub fn battlefield(&self) -> &'g [ObjectId] {
        &self.game.battlefield
    }

    /// The stack, bottom first (public, CR 400.2).
    pub fn stack(&self) -> Vec<StackEntry> {
        self.game
            .stack
            .iter()
            .map(|s| StackEntry {
                object: s.object,
                def: s.source_def,
                name: self.game.def(s.source_def).name.clone(),
                controller: s.controller,
                ability: s.ability,
                targets: s.targets.to_vec(),
            })
            .collect()
    }

    /// What the viewer's own library holds, sorted (never its order). A player
    /// knows their deck list and every card that has left the library, so the
    /// contents are theirs to deduce; the order stays hidden (CR 401.2).
    pub fn own_library_contents(&self) -> Vec<DefId> {
        let mut v: Vec<DefId> = self
            .game
            .player(self.me)
            .library
            .iter()
            .filter_map(|&id| self.game.object(id).map(|o| o.def))
            .collect();
        v.sort_unstable();
        v
    }

    /// Card definitions are public knowledge; this gives a definition by id
    /// (for reasoning about stack entries whose source has left play).
    pub fn definition(&self, def: DefId) -> &'g CardDef {
        self.game.def(def)
    }

    /// The untapped creatures `p` controls (public: they could block).
    pub fn untapped_creatures(&self, p: PlayerId) -> Vec<ObjectId> {
        self.game
            .battlefield
            .iter()
            .copied()
            .filter(|id| {
                self.game
                    .obj(*id)
                    .is_some_and(|o| o.controller == p && !o.tapped)
                    && self.game.is_creature(*id)
            })
            .collect()
    }

    pub fn snapshot(&self) -> ViewSnapshot {
        let objs = |ids: &[ObjectId]| {
            ids.iter()
                .filter_map(|id| self.object(*id))
                .collect::<Vec<_>>()
        };
        ViewSnapshot {
            me: self.me,
            turn: self.game.turn.number,
            active: self.game.turn.active,
            step: self.game.turn.step,
            players: self
                .game
                .players
                .iter()
                .map(|p| self.player(p.id))
                .collect(),
            hand: objs(self.hand()),
            battlefield: objs(&self.game.battlefield),
            stack: self.stack(),
            graveyards: self
                .game
                .players
                .iter()
                .map(|p| objs(&p.graveyard))
                .collect(),
            exile: objs(&self.game.exile),
            command: objs(&self.game.cmd.zone),
            library_top: self
                .game
                .top_card_visible(self.me)
                .and_then(|id| self.object(id)),
            controlled_hand: match self.game.controlled {
                Some((p, c)) if c == self.me && p != self.me => objs(&self.game.player(p).hand),
                _ => Vec::new(),
            },
        }
    }
}
