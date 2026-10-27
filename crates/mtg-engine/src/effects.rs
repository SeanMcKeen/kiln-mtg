//! The effect interpreter (M3 plan, B-D3): executes `mtg_cards::ir::Effect`
//! trees as a resumable procedure.
//!
//! A [`EffectTask`] holds the resolution context and a stack of [`Frame`]s.
//! Frames point into the immutable IR by *path* (child indices from the
//! ability's root effect), so clones stay cheap and the IR stays in the
//! shared card pool. A frame that needs a player's choice puts itself back
//! on the frame stack and returns a decision; the answer resumes it
//! (ADR-0004). Effects run in written order (CR 608.2c).

use std::sync::Arc;

use mtg_cards::ir::{
    Ability, ChoiceSpec, Condition, CostPart, Destination, DiscardSpec, Effect, ManaSpec,
    ObjectFilter, ObjectRef, ObjectSet, PlayerRef, Quantity, QuantityExpr, Recipient, RestOrder,
    ZoneName,
};
use mtg_core::{Color, ManaCost, ManaSymbol, ObjectId, PlayerId};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::card::{CardPool, DefId, ManaType};
use crate::cast::ManualPayTask;
use crate::decision::{CardPurpose, DecisionKind, ManaBag, OptionalKind};
use crate::engine::{Game, Task};
use crate::events::Event;
use crate::mana::{CostReq, enumerate_plans, payable};
use crate::state::{Target, ZoneId};

/// Child indices from an ability's root effect.
pub(crate) type Path = SmallVec<[u8; 6]>;

/// The ability whose effect is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct AbilityRef {
    pub def: DefId,
    /// Index into the definition's abilities.
    pub ability: u8,
}

/// Last known information about an object that left its zone (CR 608.2h).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct Lki {
    pub id: ObjectId,
    pub def: DefId,
    pub owner: PlayerId,
    pub controller: PlayerId,
    pub zone: ZoneId,
    pub power: i32,
    pub toughness: i32,
}

/// Everything an effect can refer to while it resolves.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct EffectCtx {
    pub ability: AbilityRef,
    /// The spell, or the source of the ability.
    pub source: ObjectId,
    pub controller: PlayerId,
    /// Per target slot, the targets still legal as resolution began (CR 608.2b).
    pub targets: SmallVec<[SmallVec<[Target; 1]>; 2]>,
    /// The announced X (CR 107.3).
    pub x: u32,
    /// Chosen modes, in printed order (CR 700.2).
    pub modes: SmallVec<[u8; 2]>,
    /// The player bound by "each player" (`PlayerRef::Each`).
    pub each: Option<PlayerId>,
    /// Objects named for later reference (CR 607), by the IR's name.
    pub remembered: Vec<(String, SmallVec<[ObjectId; 2]>)>,
    /// Chosen numbers and colors (color = index in `Color::ALL`).
    pub vars: Vec<(String, i64)>,
    /// Chosen card names; `None` = a name not in this game.
    pub names: Vec<(String, Option<DefId>)>,
    pub lki: Vec<Lki>,
    /// Cards each player discarded during this effect (Windfall).
    pub discarded: SmallVec<[(PlayerId, u32); 8]>,
    /// The mana a mana ability makes, fixed in advance by a payment plan or
    /// a manual payment step.
    pub forced_mana: Option<ManaBag>,
    /// A triggered ability's triggering object and player ("that card", "that player").
    pub triggering_object: Option<ObjectId>,
    pub triggering_player: Option<PlayerId>,
    /// The mana the triggering permanent produced (Kinnan, CR 605.1b).
    pub triggering_mana: Option<ManaBag>,
    /// The triggering object's ability index, when "that spell or ability"
    /// is an ability on the stack (ward).
    pub triggering_ability: Option<u8>,
    /// "That much" from the trigger event.
    pub triggering_amount: Option<u32>,
    /// This effect is a mana ability whose cost taps its source, so the
    /// mana it adds is "tapped for mana" (CR 106.12, 605.1b triggers).
    pub tapped_for_mana: bool,
    /// Replacement effects already applied to the event this effect replaces
    /// (CR 614.5, 616.1f).
    pub applied: crate::replacement::Applied,
}

impl EffectCtx {
    pub(crate) fn new(ability: AbilityRef, source: ObjectId, controller: PlayerId) -> Self {
        Self {
            ability,
            source,
            controller,
            targets: SmallVec::new(),
            x: 0,
            modes: SmallVec::new(),
            each: None,
            remembered: Vec::new(),
            vars: Vec::new(),
            names: Vec::new(),
            lki: Vec::new(),
            discarded: SmallVec::new(),
            forced_mana: None,
            triggering_object: None,
            triggering_player: None,
            triggering_mana: None,
            triggering_ability: None,
            triggering_amount: None,
            tapped_for_mana: false,
            applied: SmallVec::new(),
        }
    }
}

/// A resumable effect execution.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct EffectTask {
    pub ctx: EffectCtx,
    pub frames: Vec<Frame>,
}

impl EffectTask {
    /// Runs the ability's whole effect.
    pub(crate) fn new(ctx: EffectCtx) -> Self {
        Self {
            ctx,
            frames: vec![Frame::Run(Path::new())],
        }
    }

    /// Only pays costs (casting or activation costs, CR 601.2h, 602.2b).
    pub(crate) fn paying(ctx: EffectCtx, who: PlayerId, cost: &[CostPart]) -> Self {
        Self {
            ctx,
            frames: vec![pay_frame(who, cost)],
        }
    }

    /// Pays `cost` before the frames already queued.
    pub(crate) fn push_costs(&mut self, who: PlayerId, cost: &[CostPart]) {
        if !cost.is_empty() {
            self.frames.push(pay_frame(who, cost));
        }
    }
}

/// One step of an effect's execution.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Frame {
    /// Execute the effect node at this path.
    Run(Path),
    /// Run the body at `body` for each of `players[next..]`, binding `each`.
    ForEach {
        body: Path,
        players: SmallVec<[PlayerId; 8]>,
        next: u8,
        outer: Option<PlayerId>,
    },
    /// Ask `who` a yes/no question about the node at `node` (May, IfDoes, Unless).
    Ask {
        who: PlayerId,
        kind: OptionalKind,
        node: Path,
    },
    /// After an "if [player] does" action: run `then` or `otherwise` (node = the IfDoes).
    IfDoesThen { node: Path, did: bool },
    /// Pay costs, one part at a time (mana first, CR 601.2g).
    Pay {
        who: PlayerId,
        parts: Vec<CostPart>,
        next: u8,
    },
    /// Pick cards one at a time.
    Pick(Pick),
    /// `left` more cards for `who` to draw.
    Draws { who: PlayerId, left: u32 },
    /// Tainted Pact: the names exiled so far, and the card just exiled if
    /// its controller is being asked whether to take it.
    TaintedPact {
        names: Vec<DefId>,
        last: Option<ObjectId>,
    },
    /// Sylvan Library: pay 4 life or put `card` on top of its owner's library.
    SylvanCard { card: ObjectId },
    /// Tap or untap these objects, as the controller chooses.
    TapOrUntap { ids: Vec<ObjectId> },
    /// A creature put onto the battlefield attacking: what it attacks (CR 508.4).
    EnterAttacking { creature: ObjectId },
    /// Voting on the `Vote` at `node`: `voters[next]` votes next; `extra`
    /// when that vote is the controller's additional one (CR 701.38).
    Vote {
        node: Path,
        voters: Vec<PlayerId>,
        next: usize,
        tally: Vec<u32>,
        extra: bool,
    },
    /// Offer `chooser` to change target `index` of a spell or ability on the
    /// stack (CR 707.10c copies, 115.7d Deflecting Swat).
    ChangeTargets {
        item: Target,
        index: u8,
        chooser: PlayerId,
    },
    /// Choose which mana to add (`options` has at least two outcomes).
    Mana {
        who: PlayerId,
        options: Vec<ManaBag>,
    },
    /// Choose a value and store it.
    Choose {
        who: PlayerId,
        spec: ChoiceSpec,
        store: String,
    },
    /// Put these objects into a destination, in order.
    Place {
        ids: Vec<ObjectId>,
        to: Destination,
        who: PlayerId,
    },
    /// Return the card this Aura enchants (Animate Dead), after any
    /// "as this enters, choose" (CR 614.12).
    ReturnAttached,
}

/// Picking cards one at a time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct Pick {
    pub who: PlayerId,
    pub purpose: CardPurpose,
    pub candidates: Vec<ObjectId>,
    pub picked: Vec<ObjectId>,
    /// More picks allowed.
    pub max: u32,
    /// More picks required.
    pub min: u32,
    pub then: AfterPick,
}

/// What happens to picked cards.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum AfterPick {
    /// Each pick is placed immediately, so the pick order is the order (ordering decisions).
    Place(Destination),
    /// Scry: picks go to the bottom immediately; the rest are then ordered on top (CR 701.22a).
    ScryBottom,
    /// All picks are discarded together (CR 701.9a).
    Discard,
    /// All picks are sacrificed together (CR 701.21a).
    Sacrifice,
    /// All picks are exiled together, optionally remembered.
    Exile {
        remember: Option<String>,
        /// CR 406.3: no player may look at it.
        face_down: bool,
    },
    /// Search results (CR 701.23): reveal if asked, then put them `to`.
    Search {
        /// Who searches (CR 701.23); the pick's decider may be another
        /// player controlling them (CR 723).
        searcher: PlayerId,
        /// An Opposition Agent whose controller controls the search.
        agent: Option<ObjectId>,
        to: Destination,
        reveal: bool,
    },
    /// Put this many +1/+1 counters on the pick (amass, CR 701.47a).
    PutCounters(u32),
    /// Amass onto the pick: counters, and it becomes the subtype (CR 701.47a).
    Amass { n: u32, subtype: String },
    /// The ability's source enters the battlefield as a copy of the pick, or
    /// as itself if none (CR 707.9); `node` is the `EnterAsCopy` effect.
    EnterAsCopy { node: Path },
    /// Doomsday: every other card in `pool` is exiled, then the picks go on
    /// top of the library in the order chosen.
    Doomsday { pool: Vec<ObjectId> },
    /// Sylvan Library: each pick is kept for 4 life or put back on top.
    Sylvan,
    /// Looked-at cards taken `to`; the rest (`rest`) go per `order`.
    Take {
        to: Destination,
        rest: Vec<ObjectId>,
        order: RestOrder,
    },
}

/// The result of one step.
pub(crate) enum Flow {
    Continue,
    /// The frame put itself back; ask this.
    Decide(PlayerId, DecisionKind, &'static str),
    /// Run this task first, then resume (CR 614.11a: a replaced draw's
    /// actions finish before the sequence continues).
    Yield(Box<Task>),
}

/// The root effect of an ability.
/// Ability indices from here up name a spell's alternative-cost effect
/// (overload, CR 702.96): `ALT_EFFECT_BASE + i` is alternative cost `i`.
pub(crate) const ALT_EFFECT_BASE: u8 = 200;

/// The effect alternative cost `i` of a spell replaces its text with, if any.
pub(crate) fn alt_effect(def: &crate::card::CardDef, i: u8) -> Option<&Effect> {
    def.spell()?
        .alternative_costs
        .get(usize::from(i))?
        .effect
        .as_ref()
}

/// The ability index of a dashed permanent's delayed "return it to its
/// owner's hand" (CR 702.109a).
pub(crate) const DASH_RETURN: u8 = 255;

static DASH_RETURN_EFFECT: Effect = Effect::Move {
    what: ObjectSet::One(ObjectRef::This),
    to: Destination::Hand,
};

pub(crate) fn root(pool: &CardPool, a: AbilityRef) -> Option<&Effect> {
    if a.ability == DASH_RETURN {
        return Some(&DASH_RETURN_EFFECT);
    }
    if a.ability >= ALT_EFFECT_BASE {
        return alt_effect(pool.get(a.def), a.ability - ALT_EFFECT_BASE);
    }
    match pool.get(a.def).abilities.get(a.ability as usize)? {
        Ability::Spell(s) => Some(&s.effect),
        Ability::Activated(x) => Some(&x.effect),
        Ability::Triggered(t) => Some(&t.effect),
        Ability::Replacement(r) => Some(&r.instead),
        Ability::OpeningHand(o) => Some(&o.effect),
        _ => None,
    }
}

/// CR 400.7j: when an effect moves its own source to a public zone, later
/// parts of the effect find the new object ("this").
fn follow_source(ctx: &mut EffectCtx, old: ObjectId, new: Option<ObjectId>, to: ZoneId) {
    if let Some(new) = new
        && ctx.source == old
        && !matches!(to, ZoneId::Hand(_) | ZoneId::Library(_))
    {
        ctx.source = new;
    }
}

/// Child `i` of an effect node; the numbering matches [`Frame`] paths.
fn child(e: &Effect, i: u8) -> Option<&Effect> {
    let i = usize::from(i);
    match e {
        Effect::Seq(v) => v.get(i),
        Effect::Modal { modes, .. } => modes.get(i),
        Effect::Vote { choices, .. } => choices.get(i).map(|c| &c.per_vote),
        Effect::May(x)
        | Effect::Unless { effect: x, .. }
        | Effect::ForEachPlayer { effect: x, .. }
        | Effect::Delayed { effect: x, .. } => (i == 0).then_some(&**x),
        Effect::IfDoes {
            action,
            then,
            otherwise,
            ..
        } => match i {
            0 => Some(action),
            1 => Some(then),
            2 => otherwise.as_deref(),
            _ => None,
        },
        Effect::If {
            then, otherwise, ..
        } => match i {
            0 => Some(then),
            1 => otherwise.as_deref(),
            _ => None,
        },
        Effect::ExchangeControl { otherwise, .. } => (i == 0).then_some(otherwise.as_deref()?),
        Effect::Repeat(x) => (i == 0).then_some(&**x),
        _ => None,
    }
}

fn node<'p>(pool: &'p CardPool, a: AbilityRef, path: &[u8]) -> Option<&'p Effect> {
    path.iter().try_fold(root(pool, a)?, |e, i| child(e, *i))
}

fn with(path: &Path, i: u8) -> Path {
    let mut p = path.clone();
    p.push(i);
    p
}

/// The colors of [`Color::ALL`] as stored in variables.
fn color_index(c: Color) -> i64 {
    Color::ALL
        .iter()
        .position(|x| *x == c)
        .map_or(0, |i| i as i64)
}

/// Distinct mana outcomes of a mana spec (fixed amounts only; see `support`).
pub(crate) fn mana_bags(spec: &ManaSpec) -> Vec<ManaBag> {
    let Ok(alternatives) = crate::card::mana_output(spec) else {
        return Vec::new();
    };
    crate::card::expand_bags(&alternatives).into_vec()
}

/// A mana-only cost from its symbols (generic, colored and {C}; see `support`).
pub(crate) fn mana_req(s: &str) -> CostReq {
    s.parse::<ManaCost>()
        .map(|c| CostReq::from_cost(&c))
        .unwrap_or_default()
}

/// Whether a mana cost string uses only symbols [`CostReq`] can express.
pub(crate) fn simple_mana(s: &str) -> bool {
    s.parse::<ManaCost>().is_ok_and(|c| {
        c.symbols().iter().all(|sym| {
            matches!(
                sym,
                ManaSymbol::Generic(_) | ManaSymbol::Colored(_) | ManaSymbol::Colorless
            )
        })
    })
}

impl Game {
    // ---- the task --------------------------------------------------------

    pub(crate) fn effect_step(&mut self, mut t: Box<EffectTask>) {
        while let Some(frame) = t.frames.pop() {
            match self.exec_frame(&mut t, frame) {
                Flow::Continue => {}
                Flow::Decide(player, kind, cr) => {
                    self.tasks.push(Task::Effect(t));
                    self.decide(player, kind, cr);
                    return;
                }
                Flow::Yield(task) => {
                    self.tasks.push(Task::Effect(t));
                    self.tasks.push(*task);
                    return;
                }
            }
            if self.result.is_some() {
                return;
            }
        }
    }

    pub(crate) fn effect_answer(
        &mut self,
        mut t: Box<EffectTask>,
        kind: DecisionKind,
        option: usize,
    ) {
        let frame = t
            .frames
            .pop()
            .unwrap_or_else(|| unreachable!("an effect decision always has its frame on top"));
        // CR 903.9b: recorded, then the frame runs again and makes the move.
        if let DecisionKind::CommanderZone { commander, .. } = kind {
            self.commander_answer(commander, option == 1);
            t.frames.push(frame);
            self.tasks.push(Task::Effect(t));
            return;
        }
        // CR 614.12: likewise for "as this enters, choose".
        if matches!(frame, Frame::Place { .. } | Frame::ReturnAttached)
            && matches!(
                kind,
                DecisionKind::ChooseColor { .. } | DecisionKind::ChooseNumber { .. }
            )
        {
            self.record_entering_choice(&kind, option);
            t.frames.push(frame);
            self.tasks.push(Task::Effect(t));
            return;
        }
        match (frame, kind) {
            (Frame::Ask { kind, node, .. }, DecisionKind::Optional { .. }) => {
                let yes = option == 1;
                let pool = Arc::clone(&self.pool);
                let e = node_or_unreachable(&pool, t.ctx.ability, &node);
                match (kind, e) {
                    (OptionalKind::May, _) => {
                        if yes {
                            t.frames.push(Frame::Run(with(&node, 0)));
                        }
                    }
                    (OptionalKind::Repeat, _) => {
                        // Again: the process, then the question.
                        if yes {
                            t.frames.push(Frame::Ask {
                                who: t.ctx.controller,
                                kind: OptionalKind::Repeat,
                                node: node.clone(),
                            });
                            t.frames.push(Frame::Run(with(&node, 0)));
                        }
                    }
                    (OptionalKind::IfDoes, _) => {
                        t.frames.push(Frame::IfDoesThen {
                            node: node.clone(),
                            did: yes,
                        });
                        if yes {
                            t.frames.push(Frame::Run(with(&node, 0)));
                        }
                    }
                    (OptionalKind::PayUnless, Effect::Unless { payer, cost, .. }) => {
                        if yes {
                            let who = self.player_ref(&t.ctx, payer).unwrap_or(t.ctx.controller);
                            t.frames.push(pay_frame(who, cost));
                        } else {
                            t.frames.push(Frame::Run(with(&node, 0)));
                        }
                    }
                    (k, e) => unreachable!("optional {k:?} on {e:?}"),
                }
            }
            (Frame::Pay { who, parts, next }, DecisionKind::PayMana { plans, .. }) => {
                if let Some(plan) = plans.get(option) {
                    // The plan's activations and spending run before the effect resumes.
                    t.frames.push(Frame::Pay { who, parts, next });
                    self.tasks.push(Task::Effect(t));
                    self.execute_plan(who, plan, None);
                    return;
                } else {
                    let remaining = parts
                        .get(usize::from(next) - 1)
                        .map(|p| self.part_req(&t.ctx, p))
                        .unwrap_or_default();
                    let spell = t.ctx.source;
                    let except = taps_source(&parts).then_some(spell);
                    t.frames.push(Frame::Pay { who, parts, next });
                    self.tasks.push(Task::Effect(t));
                    self.tasks.push(Task::ManualPay(ManualPayTask {
                        spell,
                        payer: who,
                        remaining,
                        except,
                    }));
                    return;
                }
            }
            (Frame::Pick(mut p), DecisionKind::ChooseCard { cards, .. }) => match cards[option] {
                None => self.finish_pick(&mut t, p),
                Some(id) => {
                    p.candidates.retain(|c| *c != id);
                    p.max -= 1;
                    p.min = p.min.saturating_sub(1);
                    match &p.then {
                        AfterPick::Place(to) => {
                            let to = *to;
                            let who = p.who;
                            t.frames.push(Frame::Pick(p));
                            self.place_or_defer(&mut t, id, to, who);
                            self.tasks.push(Task::Effect(t));
                            return;
                        }
                        AfterPick::ScryBottom => {
                            self.place(&mut t.ctx, id, Destination::BottomOfLibrary, p.who);
                        }
                        _ => p.picked.push(id),
                    }
                    t.frames.push(Frame::Pick(p));
                }
            },
            (Frame::Mana { who, options }, DecisionKind::ChooseMana { .. }) => {
                let ctx = t.ctx.clone();
                self.mana_added(&ctx, who, &options[option]);
            }
            // CR 707.10c: option 0 keeps the target.
            (
                Frame::ChangeTargets { item, index, .. },
                DecisionKind::ChangeTarget { options, .. },
            ) => {
                if option > 0
                    && let Some(pos) = self.stack_position(item)
                    && let Some(t) = self.stack[pos].targets.get_mut(usize::from(index))
                {
                    *t = options[option - 1];
                }
            }
            (Frame::Choose { who, store, .. }, kind) => {
                let text = match kind {
                    DecisionKind::ChooseName { names, .. } => {
                        let name = names[option];
                        t.ctx.names.retain(|(k, _)| *k != store);
                        t.ctx.names.push((store, name));
                        name.map_or_else(
                            || "a name not in this game".to_owned(),
                            |d| self.def(d).name.clone(),
                        )
                    }
                    DecisionKind::ChooseNumber { min, .. } => {
                        let n = min + i64::try_from(option).unwrap_or(0);
                        set_var(&mut t.ctx, store, n);
                        n.to_string()
                    }
                    DecisionKind::ChooseColor { colors, .. } => {
                        let c = colors[option];
                        set_var(&mut t.ctx, store, color_index(c));
                        format!("{c:?}")
                    }
                    DecisionKind::ChoosePlayer { players, .. } => {
                        let p = players[option];
                        set_var(&mut t.ctx, store, i64::from(p.0));
                        p.to_string()
                    }
                    other => unreachable!("choice frame answered with {other:?}"),
                };
                self.log(Event::Chose {
                    player: who,
                    choice: text,
                });
            }
            (
                Frame::TaintedPact {
                    names,
                    last: Some(card),
                },
                DecisionKind::Optional { .. },
            ) => {
                if option == 1 {
                    let who = t.ctx.controller;
                    self.place(&mut t.ctx, card, Destination::Hand, who);
                } else {
                    t.frames.push(Frame::TaintedPact { names, last: None });
                }
            }
            (
                Frame::EnterAttacking { creature },
                DecisionKind::ChooseAttacked {
                    players,
                    planeswalkers,
                    ..
                },
            ) => {
                if let Some(&p) = players.get(option) {
                    self.enter_attacking(creature, p, None);
                } else if let Some(&pw) = planeswalkers.get(option - players.len())
                    && let Some(defending) = self.obj(pw).map(|o| o.controller)
                {
                    self.enter_attacking(creature, defending, Some(pw));
                }
            }
            (
                Frame::Vote {
                    node,
                    voters,
                    next,
                    mut tally,
                    extra,
                },
                DecisionKind::Vote { choices, .. },
            ) => {
                let voter = voters[next];
                let choice = if extra {
                    option.checked_sub(1)
                } else {
                    Some(option)
                };
                if let Some(c) = choice {
                    tally[c] += 1;
                    self.log(Event::Voted {
                        player: voter,
                        choice: choices[c].clone(),
                    });
                }
                // CR 701.38d: the controller's additional vote comes right after.
                let pool = Arc::clone(&self.pool);
                let more = !extra
                    && voter == t.ctx.controller
                    && matches!(
                        self::node(&pool, t.ctx.ability, &node),
                        Some(Effect::Vote {
                            extra_vote: true,
                            ..
                        })
                    );
                t.frames.push(Frame::Vote {
                    node,
                    voters,
                    next: if more { next } else { next + 1 },
                    tally,
                    extra: more,
                });
            }
            (Frame::TapOrUntap { ids }, DecisionKind::Optional { .. }) => {
                for id in ids {
                    if !self.on_battlefield_now(id) {
                        continue;
                    }
                    if option == 1 {
                        self.untap(id);
                    } else {
                        self.tap(id);
                    }
                }
            }
            (Frame::SylvanCard { card }, DecisionKind::Optional { .. }) => {
                let who = t.ctx.controller;
                if option == 1 {
                    self.lose_life(who, 4);
                } else {
                    self.place(&mut t.ctx, card, Destination::TopOfLibrary, who);
                }
            }
            (frame, kind) => unreachable!("effect frame {frame:?} answered with {kind:?}"),
        }
        self.tasks.push(Task::Effect(t));
    }

    fn exec_frame(&mut self, t: &mut EffectTask, frame: Frame) -> Flow {
        match frame {
            Frame::Draws { who, left } => self.draws(t, who, left),
            Frame::ChangeTargets {
                item,
                index,
                chooser,
            } => self.change_targets_step(t, item, index, chooser),
            Frame::TaintedPact { names, last } => self.tainted_pact_step(t, names, last),
            Frame::EnterAttacking { creature } => {
                let Some(o) = self.obj(creature).copied() else {
                    return Flow::Continue;
                };
                // CR 506.3b: only under the attacking player's control.
                let active = self.turn.active;
                if o.zone != ZoneId::Battlefield || o.controller != active || self.combat.is_none()
                {
                    return Flow::Continue;
                }
                // CR 802.2: every opponent is a defending player.
                let players: Vec<PlayerId> = self
                    .turn_order_from(active)
                    .into_iter()
                    .filter(|p| *p != active && self.in_game(*p))
                    .collect();
                let planeswalkers: Vec<ObjectId> = self
                    .battlefield
                    .iter()
                    .copied()
                    .filter(|id| {
                        self.obj(*id).is_some_and(|x| x.controller != active)
                            && self
                                .chars(*id)
                                .is_some_and(|c| c.is(mtg_core::CardType::Planeswalker))
                    })
                    .collect();
                match (players.as_slice(), planeswalkers.is_empty()) {
                    ([], true) => {}
                    ([only], true) => self.enter_attacking(creature, *only, None),
                    _ => {
                        t.frames.push(Frame::EnterAttacking { creature });
                        return Flow::Decide(
                            active,
                            DecisionKind::ChooseAttacked {
                                creature,
                                players,
                                planeswalkers,
                            },
                            "508.4",
                        );
                    }
                }
                Flow::Continue
            }
            Frame::Vote {
                node,
                voters,
                next,
                tally,
                extra,
            } => {
                let pool = Arc::clone(&self.pool);
                let Some(Effect::Vote { choices, .. }) = self::node(&pool, t.ctx.ability, &node)
                else {
                    return Flow::Continue;
                };
                // Players who left the game don't vote (CR 800.4a).
                let mut next = next;
                while voters.get(next).is_some_and(|p| !self.in_game(*p)) {
                    next += 1;
                }
                if let Some(&p) = voters.get(next) {
                    let names = choices.iter().map(|c| c.name.clone()).collect();
                    t.frames.push(Frame::Vote {
                        node,
                        voters,
                        next,
                        tally,
                        extra,
                    });
                    return Flow::Decide(
                        p,
                        DecisionKind::Vote {
                            source: t.ctx.source,
                            choices: names,
                            extra,
                        },
                        "701.38a",
                    );
                }
                // Each vote's effect, in the listed order.
                for (i, n) in tally.iter().enumerate().rev() {
                    let Ok(i) = u8::try_from(i) else { continue };
                    for _ in 0..*n {
                        t.frames.push(Frame::Run(with(&node, i)));
                    }
                }
                Flow::Continue
            }
            Frame::TapOrUntap { ids } => {
                let source = ids[0];
                t.frames.push(Frame::TapOrUntap { ids });
                Flow::Decide(
                    t.ctx.controller,
                    DecisionKind::Optional {
                        source,
                        kind: OptionalKind::TapOrUntap,
                    },
                    "608.2d",
                )
            }
            Frame::SylvanCard { card } => {
                let who = t.ctx.controller;
                if self.obj(card).is_none_or(|o| o.zone != ZoneId::Hand(who)) {
                    return Flow::Continue;
                }
                // CR 119.4: 4 life can be paid only with at least 4 life.
                if self.player(who).life < 4 {
                    self.place(&mut t.ctx, card, Destination::TopOfLibrary, who);
                    return Flow::Continue;
                }
                t.frames.push(Frame::SylvanCard { card });
                Flow::Decide(
                    who,
                    DecisionKind::Optional {
                        source: card,
                        kind: OptionalKind::PayUnless,
                    },
                    "119.4",
                )
            }
            Frame::Run(path) => {
                let pool = Arc::clone(&self.pool);
                match node(&pool, t.ctx.ability, &path) {
                    Some(e) => self.exec(t, &path, e),
                    None => Flow::Continue,
                }
            }
            Frame::ForEach {
                body,
                players,
                next,
                outer,
            } => {
                if let Some(&p) = players.get(usize::from(next)) {
                    t.frames.push(Frame::ForEach {
                        body: body.clone(),
                        players,
                        next: next + 1,
                        outer,
                    });
                    t.ctx.each = Some(p);
                    t.frames.push(Frame::Run(body));
                } else {
                    t.ctx.each = outer;
                }
                Flow::Continue
            }
            Frame::Ask { who, kind, node } => {
                let source = t.ctx.source;
                t.frames.push(Frame::Ask { who, kind, node });
                Flow::Decide(who, DecisionKind::Optional { source, kind }, "608.2d")
            }
            Frame::IfDoesThen { node, did } => {
                t.frames
                    .push(Frame::Run(with(&node, if did { 1 } else { 2 })));
                Flow::Continue
            }
            Frame::Pay { who, parts, next } => self.pay_step(t, who, parts, next),
            Frame::ReturnAttached => {
                let controller = t.ctx.controller;
                // CR 614.12: "as this enters, choose" before it's put onto the battlefield.
                if let Some(Target::Object(card)) = self.attached_to(t.ctx.source)
                    && self
                        .obj(card)
                        .is_some_and(|o| matches!(o.zone, ZoneId::Graveyard(_)))
                    && let Some((p, kind)) = self.entering_question(card, controller)
                {
                    t.frames.push(Frame::ReturnAttached);
                    return Flow::Decide(p, kind, "614.12");
                }
                self.return_attached(t.ctx.source, controller);
                Flow::Continue
            }
            Frame::Place { ids, to, who } => {
                // CR 903.9b: each commander's owner answers before the move.
                if let Some((owner, kind)) =
                    self.commander_question(ids.iter().copied(), |id| self.destination_zone(id, to))
                {
                    t.frames.push(Frame::Place { ids, to, who });
                    return Flow::Decide(owner, kind, "903.9b");
                }
                // CR 614.12: "as this enters, choose" before it's put onto the battlefield.
                if matches!(
                    to,
                    Destination::Battlefield | Destination::BattlefieldTapped
                ) && let Some((p, kind)) =
                    ids.iter().find_map(|id| self.entering_question(*id, who))
                {
                    t.frames.push(Frame::Place { ids, to, who });
                    return Flow::Decide(p, kind, "614.12");
                }
                for id in ids {
                    self.place(&mut t.ctx, id, to, who);
                }
                Flow::Continue
            }
            Frame::Pick(p) => self.pick_step(t, p),
            Frame::Mana { who, options } => {
                let source = t.ctx.source;
                t.frames.push(Frame::Mana {
                    who,
                    options: options.clone(),
                });
                Flow::Decide(who, DecisionKind::ChooseMana { source, options }, "106.4")
            }
            Frame::Choose { who, spec, store } => {
                let source = t.ctx.source;
                let kind = match &spec {
                    ChoiceSpec::CardName => DecisionKind::ChooseName {
                        source,
                        names: self.nameable(),
                    },
                    ChoiceSpec::Color => DecisionKind::ChooseColor {
                        source,
                        colors: Color::ALL.to_vec(),
                    },
                    ChoiceSpec::Number { min, max } => DecisionKind::ChooseNumber {
                        source,
                        min: *min,
                        max: *max,
                    },
                    ChoiceSpec::Opponent => {
                        let players: Vec<PlayerId> = self
                            .players
                            .iter()
                            .filter(|p| p.in_game() && p.id != who)
                            .map(|p| p.id)
                            .collect();
                        if players.is_empty() {
                            return Flow::Continue;
                        }
                        DecisionKind::ChoosePlayer { source, players }
                    }
                };
                t.frames.push(Frame::Choose { who, spec, store });
                Flow::Decide(who, kind, "608.2d")
            }
        }
    }

    /// CR 201.4: names of cards (never tokens) in this game's pool, then "a name not in this game".
    fn nameable(&self) -> Vec<Option<DefId>> {
        let mut v: Vec<Option<DefId>> = self
            .pool
            .iter()
            .filter(|(_, d)| !d.token)
            .map(|(id, _)| Some(id))
            .collect();
        v.push(None);
        v
    }

    // ---- executing one node ----------------------------------------------

    #[allow(clippy::too_many_lines)]
    fn exec(&mut self, t: &mut EffectTask, path: &Path, e: &Effect) -> Flow {
        match e {
            Effect::Seq(v) => {
                for i in (0..v.len()).rev() {
                    t.frames
                        .push(Frame::Run(with(path, u8::try_from(i).unwrap_or(u8::MAX))));
                }
            }
            Effect::Modal { .. } => {
                for m in t.ctx.modes.clone().into_iter().rev() {
                    t.frames.push(Frame::Run(with(path, m)));
                }
            }
            Effect::May(_) => {
                t.frames.push(Frame::Ask {
                    who: t.ctx.controller,
                    kind: OptionalKind::May,
                    node: path.clone(),
                });
            }
            Effect::IfDoes { who, action, .. } => {
                let who = self.player_ref(&t.ctx, who).unwrap_or(t.ctx.controller);
                if self.can_do(&t.ctx, who, action) {
                    t.frames.push(Frame::Ask {
                        who,
                        kind: OptionalKind::IfDoes,
                        node: path.clone(),
                    });
                } else {
                    t.frames.push(Frame::IfDoesThen {
                        node: path.clone(),
                        did: false,
                    });
                }
            }
            Effect::If { condition, .. } => {
                let holds = self.condition(&t.ctx, condition);
                t.frames
                    .push(Frame::Run(with(path, if holds { 0 } else { 1 })));
            }
            Effect::Unless { payer, cost, .. } => {
                // CR 118.12: the payer chooses whether to pay, if they can.
                match self.player_ref(&t.ctx, payer) {
                    Some(who) if self.can_pay(&t.ctx, who, cost) => {
                        t.frames.push(Frame::Ask {
                            who,
                            kind: OptionalKind::PayUnless,
                            node: path.clone(),
                        });
                    }
                    _ => t.frames.push(Frame::Run(with(path, 0))),
                }
            }
            Effect::ForEachPlayer { players, .. } => {
                // CR 101.4: APNAP order.
                let players = self
                    .turn_order_from(self.turn.active)
                    .into_iter()
                    .filter(|p| self.player_rel(*players, t.ctx.controller, *p))
                    .collect();
                t.frames.push(Frame::ForEach {
                    body: with(path, 0),
                    players,
                    next: 0,
                    outer: t.ctx.each,
                });
            }
            Effect::Pay { who, cost } => {
                if let Some(who) = self.player_ref(&t.ctx, who)
                    && self.can_pay(&t.ctx, who, cost)
                {
                    t.frames.push(pay_frame(who, cost));
                }
            }
            Effect::DealDamage { to, amount } => {
                let amount = self.amount(&t.ctx, amount);
                let source = t.ctx.source;
                for target in self.recipients(&t.ctx, to) {
                    self.deal_damage(source, target, amount, false);
                }
            }
            Effect::Destroy(set) => {
                // The destroyed permanents leave simultaneously (CR 603.10a).
                let ids = self.set_now(&t.ctx, set);
                self.begin_leaving(&ids);
                for id in ids {
                    self.destroy(&mut t.ctx, id);
                }
                self.end_leaving();
            }
            Effect::Exile {
                what,
                remember,
                face_down,
            } => {
                let remember = remember.clone();
                let face_down = *face_down;
                if let Some(p) = self.pick_for(t, what, CardPurpose::Exile) {
                    t.frames.push(Frame::Pick(Pick {
                        then: AfterPick::Exile {
                            remember,
                            face_down,
                        },
                        ..p
                    }));
                    return Flow::Continue;
                }
                let ids = self.set_now(&t.ctx, what);
                self.exile_all(&mut t.ctx, &ids, remember, face_down);
            }
            Effect::Move { what, to } => {
                let to = *to;
                if let Some(p) = self.pick_for(t, what, purpose_for(to)) {
                    t.frames.push(Frame::Pick(Pick {
                        then: AfterPick::Place(to),
                        ..p
                    }));
                    return Flow::Continue;
                }
                let who = t.ctx.each.unwrap_or(t.ctx.controller);
                // One simultaneous move; commander owners answer first (CR 903.9b).
                let ids = self.set_now(&t.ctx, what);
                t.frames.push(Frame::Place { ids, to, who });
            }
            Effect::Pump {
                what,
                power,
                toughness,
            } => {
                // CR 611.2c: the affected set is locked in now.
                let affected: SmallVec<[ObjectId; 4]> = self
                    .set_now(&t.ctx, what)
                    .into_iter()
                    .filter(|id| self.on_battlefield_now(*id))
                    .collect();
                if !affected.is_empty() {
                    let clamp = |v: i64| {
                        i32::try_from(v).unwrap_or(if v < 0 { i32::MIN } else { i32::MAX })
                    };
                    let (power, toughness) = (
                        clamp(self.quantity(&t.ctx, power)),
                        clamp(self.quantity(&t.ctx, toughness)),
                    );
                    // CR 611.2a: until end of turn, ending in cleanup (CR 514.2).
                    self.add_effect(
                        affected,
                        crate::layers::LayerEffect::ModifyPt { power, toughness },
                        crate::layers::Duration::EndOfTurn,
                    );
                }
            }
            Effect::Reveal(what) => {
                let objects = self.set_now(&t.ctx, what);
                if !objects.is_empty() {
                    let player = self.obj(objects[0]).map_or(t.ctx.controller, |o| o.owner);
                    self.log(Event::Revealed { player, objects });
                }
            }
            Effect::RevealUntil {
                who,
                filter,
                found_to,
                rest_to,
            } => {
                if let Some(p) = self.player_ref(&t.ctx, who) {
                    self.reveal_until(t, p, filter, *found_to, *rest_to);
                }
            }
            Effect::Sacrifice { who, what } => {
                let Some(who) = self.player_ref(&t.ctx, who) else {
                    return Flow::Continue;
                };
                if let ObjectSet::ChosenFrom { filter, count, .. } = what {
                    let count = self.amount(&t.ctx, count);
                    let candidates = self.own_permanents(&t.ctx, who, filter);
                    t.frames.push(Frame::Pick(exact_pick(
                        who,
                        CardPurpose::Sacrifice,
                        candidates,
                        count,
                        AfterPick::Sacrifice,
                    )));
                    return Flow::Continue;
                }
                let ids: Vec<ObjectId> = self
                    .set_now(&t.ctx, what)
                    .into_iter()
                    .filter(|id| {
                        self.obj(*id)
                            .is_some_and(|o| o.zone == ZoneId::Battlefield && o.controller == who)
                    })
                    .collect();
                self.sacrifice_all(&mut t.ctx, &ids);
            }
            Effect::Counter(set) => {
                // "Counter that spell or ability" when it is an ability (ward).
                if let (ObjectSet::One(ObjectRef::Triggering), Some(source), Some(ability)) =
                    (set, t.ctx.triggering_object, t.ctx.triggering_ability)
                {
                    self.counter_ability(source, ability);
                    return Flow::Continue;
                }
                // Targeted abilities on the stack (Stifle, CR 701.6b).
                if let ObjectSet::One(ObjectRef::Target(n)) = set {
                    let abilities: Vec<Target> = t
                        .ctx
                        .targets
                        .get(usize::from(*n))
                        .map(|slot| {
                            slot.iter()
                                .copied()
                                .filter(|x| matches!(x, Target::StackAbility { .. }))
                                .collect()
                        })
                        .unwrap_or_default();
                    for a in abilities {
                        if let Some(pos) = self.stack_position(a) {
                            let item = self.stack.remove(pos);
                            self.log(Event::Countered {
                                object: item.object,
                            });
                        }
                    }
                }
                for id in self.set_now(&t.ctx, set) {
                    self.counter(&mut t.ctx, id);
                }
            }
            Effect::CounterAndExile(set) => {
                for id in self.set_now(&t.ctx, set) {
                    self.counter_to(&mut t.ctx, id, true);
                }
            }
            Effect::LookAtHand(who) => {
                // No game state changes; the look is logged (who saw whose hand).
                if let Some(p) = self.player_ref(&t.ctx, who) {
                    self.log(Event::LookedAtHand {
                        viewer: t.ctx.controller,
                        player: p,
                    });
                }
            }
            Effect::Draw { who, amount } => {
                let n = self.amount(&t.ctx, amount);
                if let Some(p) = self.player_ref(&t.ctx, who) {
                    return self.draws(t, p, n);
                }
            }
            Effect::Discard { who, what } => {
                let Some(p) = self.player_ref(&t.ctx, who) else {
                    return Flow::Continue;
                };
                self.discard(t, p, what);
            }
            Effect::GainLife { who, amount } => {
                let n = self.amount(&t.ctx, amount);
                if let Some(p) = self.player_ref(&t.ctx, who) {
                    self.gain_life(p, n);
                }
            }
            Effect::LoseLife { who, amount } => {
                let n = self.amount(&t.ctx, amount);
                if let Some(p) = self.player_ref(&t.ctx, who) {
                    self.lose_life(p, n);
                }
            }
            Effect::AddMana { mana, to, .. } => {
                let who = match to {
                    Some(r) => match self.player_ref(&t.ctx, r) {
                        Some(p) => p,
                        None => return Flow::Continue,
                    },
                    None => t.ctx.each.unwrap_or(t.ctx.controller),
                };
                let options = match (t.ctx.forced_mana, mana) {
                    (Some(bag), _) => vec![bag],
                    (None, ManaSpec::Colorless(q)) => {
                        let n = u8::try_from(self.amount(&t.ctx, q)).unwrap_or(u8::MAX);
                        vec![[0, 0, 0, 0, 0, n]]
                    }
                    // One mana of any type the triggering permanent produced.
                    (None, ManaSpec::TypeProducedByTrigger) => t
                        .ctx
                        .triggering_mana
                        .map(|bag| {
                            ManaType::ALL
                                .into_iter()
                                .filter(|m| bag[m.index()] > 0)
                                .map(|m| {
                                    let mut b = [0; 6];
                                    b[m.index()] = 1;
                                    b
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    (None, spec) if crate::card::is_dynamic_spec(spec) => {
                        self.dynamic_mana_bags(spec, &t.ctx)
                    }
                    (None, _) => mana_bags(mana),
                };
                match options.as_slice() {
                    [] => {}
                    [only] => self.mana_added(&t.ctx, who, only),
                    _ => t.frames.push(Frame::Mana { who, options }),
                }
            }
            Effect::Search {
                who,
                filter,
                count,
                to,
                reveal,
            } => {
                let Some(p) = self.player_ref(&t.ctx, who) else {
                    return Flow::Continue;
                };
                let count = self.amount(&t.ctx, count);
                let library = self.player(p).library.clone();
                // Aven Mindcensor: an opponent of its controller searches the top four.
                let top_four = self.active_rule_statics().iter().any(|(_, controller, e)| {
                    *controller != p
                        && matches!(
                            e,
                            mtg_cards::ir::StaticEffect::Rule(
                                mtg_cards::ir::RuleMod::OpponentsSearchTopFour
                            )
                        )
                });
                let searched = if top_four { 4 } else { library.len() };
                let candidates: Vec<ObjectId> = library
                    .iter()
                    .rev()
                    .take(searched)
                    .copied()
                    .filter(|id| self.matches_card(&t.ctx, *id, filter))
                    .collect();
                // CR 701.23b/d: a search for a stated quality may fail to find;
                // a search for a quantity of cards must find as many as possible.
                let quality = *filter != ObjectFilter::default();
                let n = count.min(u32::try_from(candidates.len()).unwrap_or(u32::MAX));
                // CR 723.5: a player controlling the searcher makes the choices.
                let agent = self.search_controller(p);
                t.frames.push(Frame::Pick(Pick {
                    who: agent.map_or(p, |(_, q)| q),
                    purpose: CardPurpose::Search,
                    candidates,
                    picked: Vec::new(),
                    max: n,
                    min: if quality { 0 } else { n },
                    then: AfterPick::Search {
                        searcher: p,
                        agent: agent.map(|(a, _)| a),
                        to: *to,
                        reveal: *reveal,
                    },
                }));
            }
            Effect::Shuffle(who) => {
                if let Some(p) = self.player_ref(&t.ctx, who) {
                    self.shuffle_library(p);
                }
            }
            Effect::Scry { who, amount } => {
                let Some(p) = self.player_ref(&t.ctx, who) else {
                    return Flow::Continue;
                };
                let n = self.amount(&t.ctx, amount);
                let looked = self.top_cards(p, n);
                self.log(Event::Scried {
                    player: p,
                    count: u32::try_from(looked.len()).unwrap_or(0),
                });
                let len = u32::try_from(looked.len()).unwrap_or(0);
                t.frames.push(Frame::Pick(Pick {
                    who: p,
                    purpose: CardPurpose::PutOnBottom,
                    candidates: looked,
                    picked: Vec::new(),
                    max: len,
                    min: 0,
                    then: AfterPick::ScryBottom,
                }));
            }
            Effect::LookAtTop {
                who,
                count,
                take,
                rest,
            } => {
                let Some(p) = self.player_ref(&t.ctx, who) else {
                    return Flow::Continue;
                };
                let n = self.amount(&t.ctx, count);
                let looked = self.top_cards(p, n);
                match take {
                    Some(take) => {
                        let candidates: Vec<ObjectId> = looked
                            .iter()
                            .copied()
                            .filter(|id| self.matches_card(&t.ctx, *id, &take.filter))
                            .collect();
                        let max = take
                            .up_to
                            .min(u32::try_from(candidates.len()).unwrap_or(u32::MAX));
                        t.frames.push(Frame::Pick(Pick {
                            who: p,
                            purpose: CardPurpose::Take,
                            candidates,
                            picked: Vec::new(),
                            max,
                            min: 0,
                            then: AfterPick::Take {
                                to: take.to,
                                rest: looked,
                                order: *rest,
                            },
                        }));
                    }
                    None => self.order_rest(t, p, looked, *rest),
                }
            }
            Effect::MoveCards {
                who,
                from,
                count,
                to,
            } => {
                let Some(p) = self.player_ref(&t.ctx, who) else {
                    return Flow::Continue;
                };
                let n = self.amount(&t.ctx, count);
                let candidates = match from {
                    ZoneName::Hand => self.player(p).hand.clone(),
                    ZoneName::Graveyard => self.player(p).graveyard.clone(),
                    _ => Vec::new(),
                };
                let n = n.min(u32::try_from(candidates.len()).unwrap_or(u32::MAX));
                t.frames.push(Frame::Pick(Pick {
                    who: p,
                    purpose: purpose_for(*to),
                    candidates,
                    picked: Vec::new(),
                    max: n,
                    min: n,
                    then: AfterPick::Place(*to),
                }));
            }
            Effect::Tap(set) => {
                for id in self.set_now(&t.ctx, set) {
                    if self.on_battlefield_now(id) {
                        self.tap(id);
                    }
                }
            }
            Effect::Untap(set) => {
                for id in self.set_now(&t.ctx, set) {
                    if self.on_battlefield_now(id) {
                        self.untap(id);
                    }
                }
            }
            Effect::EndTheTurn => {
                t.frames.clear();
                self.end_the_turn();
            }
            Effect::Flicker(set) => {
                let mut back: Vec<(ObjectId, PlayerId)> = Vec::new();
                for id in self.set_now(&t.ctx, set) {
                    if !self.on_battlefield_now(id) {
                        continue;
                    }
                    let Some(owner) = self.obj(id).map(|o| o.owner) else {
                        continue;
                    };
                    if let Some(new) = self.fx_move(&mut t.ctx, id, ZoneId::Exile, true) {
                        back.push((new, owner));
                    }
                }
                // Under each owner's control; as-enters choices are asked (CR 614.12).
                for (new, owner) in back.into_iter().rev() {
                    t.frames.push(Frame::Place {
                        ids: vec![new],
                        to: Destination::Battlefield,
                        who: owner,
                    });
                }
            }
            Effect::Become {
                what,
                base_pt,
                add_subtypes,
            } => {
                // CR 611.2c: the affected set is locked in now.
                let affected: SmallVec<[ObjectId; 4]> = self
                    .set_now(&t.ctx, what)
                    .into_iter()
                    .filter(|id| self.on_battlefield_now(*id))
                    .collect();
                if !affected.is_empty() {
                    if let Some([power, toughness]) = *base_pt {
                        self.add_effect(
                            affected.clone(),
                            crate::layers::LayerEffect::SetBasePt { power, toughness },
                            crate::layers::Duration::EndOfTurn,
                        );
                    }
                    for st in add_subtypes {
                        self.add_effect(
                            affected.clone(),
                            crate::layers::LayerEffect::AddSubtype(st.clone()),
                            crate::layers::Duration::EndOfTurn,
                        );
                    }
                }
            }
            Effect::CopyTriggeringAbility { new_targets } => {
                let who = t.ctx.controller;
                if let (Some(source), Some(ability)) =
                    (t.ctx.triggering_object, t.ctx.triggering_ability)
                    && let Some(copy) = self.copy_ability(source, ability, who)
                    && *new_targets
                {
                    // CR 707.10c: the copy's controller may choose new targets.
                    t.frames.push(Frame::ChangeTargets {
                        item: copy,
                        index: 0,
                        chooser: who,
                    });
                }
            }
            Effect::TapOrUntap(set) => {
                let ids: Vec<ObjectId> = self
                    .set_now(&t.ctx, set)
                    .into_iter()
                    .filter(|id| self.on_battlefield_now(*id))
                    .collect();
                if !ids.is_empty() {
                    t.frames.push(Frame::TapOrUntap { ids });
                }
            }
            Effect::Choose { what, store } => {
                t.frames.push(Frame::Choose {
                    who: t.ctx.each.unwrap_or(t.ctx.controller),
                    spec: what.clone(),
                    store: store.clone(),
                });
            }
            Effect::WinGame(who) => {
                if let Some(p) = self.player_ref(&t.ctx, who) {
                    self.win_by_effect(p);
                }
            }
            Effect::PhaseOut { what } => {
                for id in self.set_now(&t.ctx, what) {
                    self.phase_out(id);
                }
            }
            Effect::PlayerRule {
                who,
                rule,
                until_end_of_turn,
            } => {
                if let Some(p) = self.player_ref(&t.ctx, who) {
                    if *until_end_of_turn {
                        self.player_rules_eot.push((p, *rule));
                    } else {
                        // "Until your next turn": ends as the controller's next turn begins.
                        let until = t.ctx.controller;
                        self.player_rules.push((p, *rule, until));
                    }
                }
            }
            Effect::Attach { to } => {
                if let Some(&host) = self.object_refs(&t.ctx, to).first() {
                    self.attach_source(t.ctx.source, host);
                }
            }
            Effect::ReturnAttached => t.frames.push(Frame::ReturnAttached),
            Effect::GainControl {
                what,
                until_end_of_turn,
                to,
            } => {
                let p = match to {
                    Some(r) => match self.player_ref(&t.ctx, r) {
                        Some(p) => p,
                        None => return Flow::Continue,
                    },
                    None => t.ctx.controller,
                };
                let duration = if *until_end_of_turn {
                    crate::layers::Duration::EndOfTurn
                } else {
                    crate::layers::Duration::Permanent
                };
                for id in self.set_now(&t.ctx, what) {
                    if self.on_battlefield_now(id) {
                        self.gain_control(id, p, duration);
                    }
                }
            }
            Effect::GainKeyword { what, keyword } => {
                let affected: SmallVec<[ObjectId; 4]> = self
                    .set_now(&t.ctx, what)
                    .into_iter()
                    .filter(|id| self.on_battlefield_now(*id))
                    .collect();
                if !affected.is_empty() {
                    self.add_effect(
                        affected,
                        crate::layers::LayerEffect::GrantKeyword(Box::new(keyword.clone())),
                        crate::layers::Duration::EndOfTurn,
                    );
                }
            }
            Effect::EnterAsCopy { of, .. } => {
                // CR 707.9, 614.12a: choose as it would enter ("you may").
                let this = t.ctx.source;
                let you = t.ctx.controller;
                let candidates: Vec<ObjectId> = self
                    .battlefield
                    .iter()
                    .copied()
                    .filter(|id| *id != this && self.matches_filter(*id, of, you, Some(this)))
                    .collect();
                t.frames.push(Frame::Pick(Pick {
                    who: you,
                    purpose: CardPurpose::Copy,
                    candidates,
                    picked: Vec::new(),
                    max: 1,
                    min: 0,
                    then: AfterPick::EnterAsCopy { node: path.clone() },
                }));
            }
            Effect::CreateTokenCopy {
                of,
                except,
                remember,
            } => {
                let you = t.ctx.controller;
                let granter = t.ctx.ability.def;
                let mut made: SmallVec<[ObjectId; 2]> = SmallVec::new();
                for id in self.set_now(&t.ctx, of) {
                    if let Some(cv) = self.copy_values_with(id, except, granter)
                        && let Some(token) = self.create_token_copy(cv, you)
                    {
                        made.push(token);
                    }
                }
                if let Some(name) = remember {
                    t.ctx.remembered.retain(|(k, _)| k != name);
                    t.ctx.remembered.push((name.clone(), made));
                }
            }
            Effect::CopySpell {
                what,
                new_targets,
                count,
                controller,
            } => {
                let n = count.as_ref().map_or(1, |q| self.amount(&t.ctx, q));
                let owner = match controller {
                    Some(r) => match self.player_ref(&t.ctx, r) {
                        Some(p) => p,
                        None => return Flow::Continue,
                    },
                    None => t.ctx.controller,
                };
                for id in self.set_now(&t.ctx, what) {
                    for _ in 0..n {
                        if let Some(copy) = self.copy_spell(id, owner)
                            && *new_targets
                        {
                            // CR 707.10c: the copy's controller may choose new targets.
                            t.frames.push(Frame::ChangeTargets {
                                item: Target::Object(copy),
                                index: 0,
                                chooser: owner,
                            });
                        }
                    }
                }
            }
            Effect::Repeat(_) => {
                t.frames.push(Frame::Ask {
                    who: t.ctx.controller,
                    kind: OptionalKind::Repeat,
                    node: path.clone(),
                });
                t.frames.push(Frame::Run(with(path, 0)));
            }
            Effect::MayCastThisTurn(what) => {
                for id in self.set_now(&t.ctx, what) {
                    self.cast_permissions.push((id, t.ctx.controller));
                }
            }
            Effect::ChangeTargets(r) => {
                let items: Vec<Target> = match r {
                    ObjectRef::Target(n) => t
                        .ctx
                        .targets
                        .get(usize::from(*n))
                        .map(|slot| slot.to_vec())
                        .unwrap_or_default(),
                    other => self
                        .object_refs(&t.ctx, other)
                        .into_iter()
                        .map(Target::Object)
                        .collect(),
                };
                for item in items.into_iter().rev() {
                    t.frames.push(Frame::ChangeTargets {
                        item,
                        index: 0,
                        chooser: t.ctx.controller,
                    });
                }
            }
            Effect::CopyCard { what, remember } => {
                let ids = self.set_now(&t.ctx, what);
                let copies: SmallVec<[ObjectId; 2]> = ids
                    .iter()
                    .filter_map(|id| self.copy_card(*id, t.ctx.controller))
                    .collect();
                t.ctx.remembered.retain(|(k, _)| k != remember);
                t.ctx.remembered.push((remember.clone(), copies));
            }
            Effect::Cast { what, .. } => {
                let p = t.ctx.controller;
                if let Some(&id) = self.set_now(&t.ctx, what).first()
                    && let Some(task) = self.cast_during_resolution(p, id)
                {
                    return Flow::Yield(Box::new(task));
                }
            }
            Effect::ExchangeControl { a, b, otherwise } => {
                // CR 701.12a-b: both permanents, controlled by different
                // players who are still in the game; otherwise nothing changes.
                let x = self.object_refs(&t.ctx, a).first().copied();
                let y = self.object_refs(&t.ctx, b).first().copied();
                let pair = x.zip(y).filter(|(x, y)| {
                    self.on_battlefield_now(*x) && self.on_battlefield_now(*y) && x != y
                });
                let controllers = pair.and_then(|(x, y)| {
                    let (cx, cy) = (self.obj(x)?.controller, self.obj(y)?.controller);
                    (cx != cy && self.in_game(cx) && self.in_game(cy)).then_some((cx, cy))
                });
                match (pair, controllers) {
                    (Some((x, y)), Some((cx, cy))) => {
                        self.gain_control(x, cy, crate::layers::Duration::Permanent);
                        self.gain_control(y, cx, crate::layers::Duration::Permanent);
                    }
                    _ => {
                        if otherwise.is_some() {
                            t.frames.push(Frame::Run(with(path, 0)));
                        }
                    }
                }
            }
            Effect::LoseGame(who) => {
                // CR 104.3e
                if let Some(p) = self.player_ref(&t.ctx, who) {
                    self.players_lose(&[(p, crate::state::LossReason::Effect)]);
                }
            }
            Effect::Delayed { at, .. } => {
                // CR 603.7a: created now, with this effect's context.
                self.delayed.push(crate::triggers::DelayedTrigger {
                    trigger: at.clone(),
                    path: with(path, 0),
                    ctx: t.ctx.clone(),
                });
            }
            Effect::Amass { subtype, amount } => {
                return self.amass(t, subtype, amount);
            }
            Effect::PutCounters {
                what,
                counter,
                amount,
            } => {
                let n = self.amount(&t.ctx, amount);
                for id in self.set_now(&t.ctx, what) {
                    self.add_counters(id, *counter, n);
                }
            }
            Effect::RemoveCounters {
                what,
                counter,
                amount,
            } => {
                let n = self.amount(&t.ctx, amount);
                for id in self.set_now(&t.ctx, what) {
                    self.remove_counters(id, *counter, n);
                }
            }
            Effect::CreateToken {
                token,
                count,
                controller,
                attacking,
            } => {
                let n = self.amount(&t.ctx, count);
                if let (Some(def), Some(p)) =
                    (self.pool.token(token), self.player_ref(&t.ctx, controller))
                {
                    let made = self.create_tokens(def, p, n);
                    if *attacking {
                        // "Tapped and attacking": it enters tapped (not "becomes
                        // tapped"), then its controller chooses what it attacks.
                        for &id in made.iter().rev() {
                            if let Some(o) = self.objects.get_mut(id) {
                                o.tapped = true;
                            }
                            t.frames.push(Frame::EnterAttacking { creature: id });
                        }
                    }
                }
            }
            Effect::AdditionalCombatPhase => self.extra_combats += 1,
            Effect::Vote { choices, .. } => {
                // CR 701.38a: starting with the controller, in turn order.
                let voters = self.turn_order_from(t.ctx.controller).to_vec();
                t.frames.push(Frame::Vote {
                    node: path.clone(),
                    voters,
                    next: 0,
                    tally: vec![0; choices.len()],
                    extra: false,
                });
            }
            Effect::Escape(x) => {
                // `support::check` admits only registered escapes.
                if let Some(f) = crate::escapes::lookup(&x.function) {
                    f(self, t);
                }
            }
            // `support::check` rejects these until their phase lands.
            Effect::EntersTapped | Effect::EntersWithCounters { .. } => {
                debug_assert!(false, "unsupported effect reached the interpreter: {e:?}");
            }
        }
        Flow::Continue
    }

    // ---- references, quantities, conditions -----------------------------

    /// The objects a reference denotes (a target slot can hold several, "up to N").
    fn object_refs(&self, ctx: &EffectCtx, r: &ObjectRef) -> SmallVec<[ObjectId; 2]> {
        match r {
            ObjectRef::This => SmallVec::from_slice(&[ctx.source]),
            // "Enchanted creature" (CR 303.4b), also after the source left (LKI).
            ObjectRef::AttachedTo => match self.attached_to(ctx.source) {
                Some(Target::Object(h)) => SmallVec::from_slice(&[h]),
                _ => SmallVec::new(),
            },
            ObjectRef::Target(n) => ctx
                .targets
                .get(usize::from(*n))
                .map(|slot| {
                    slot.iter()
                        .filter_map(|t| match t {
                            Target::Object(id) => Some(*id),
                            Target::Player(_) | Target::StackAbility { .. } => None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            // CR 607.2a: otherwise, objects a linked ability of the source remembered.
            ObjectRef::Remembered(name) => ctx
                .remembered
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
                .or_else(|| {
                    self.linked
                        .iter()
                        .find(|(s, k, _)| *s == ctx.source && k == name)
                        .map(|(_, _, v)| v.clone())
                })
                .unwrap_or_default(),
            ObjectRef::Triggering => ctx.triggering_object.into_iter().collect(),
        }
    }

    fn object_ref(&self, ctx: &EffectCtx, r: &ObjectRef) -> Option<ObjectId> {
        self.object_refs(ctx, r).first().copied()
    }

    /// A player reference; `None` if it denotes nobody (an illegal target, CR 608.2b).
    pub(crate) fn player_ref(&self, ctx: &EffectCtx, r: &PlayerRef) -> Option<PlayerId> {
        let p = match r {
            PlayerRef::You => Some(ctx.controller),
            PlayerRef::Target(n) => ctx.targets.get(usize::from(*n)).and_then(|slot| {
                slot.iter().find_map(|t| match t {
                    Target::Player(p) => Some(*p),
                    Target::Object(_) | Target::StackAbility { .. } => None,
                })
            }),
            PlayerRef::ControllerOf(o) => self
                .object_ref(ctx, o)
                .and_then(|id| self.known(ctx, id).map(|k| k.controller)),
            PlayerRef::OwnerOf(o) => self
                .object_ref(ctx, o)
                .and_then(|id| self.known(ctx, id).map(|k| k.owner)),
            PlayerRef::Each => ctx.each,
            PlayerRef::Triggering => ctx.triggering_player,
            PlayerRef::Chosen(name) => ctx
                .vars
                .iter()
                .find(|(k, _)| k == name)
                .and_then(|(_, v)| u8::try_from(*v).ok())
                .map(PlayerId),
        };
        p.filter(|p| self.in_game(*p))
    }

    /// Current information about an object, or its last known information (CR 608.2h).
    fn known(&self, ctx: &EffectCtx, id: ObjectId) -> Option<Lki> {
        if let Some(o) = self.obj(id) {
            return Some(Lki {
                id,
                def: o.def,
                owner: o.owner,
                controller: o.controller,
                zone: o.zone,
                power: self.power(id),
                toughness: self.toughness(id),
            });
        }
        ctx.lki.iter().find(|l| l.id == id).copied()
    }

    fn recipients(&self, ctx: &EffectCtx, r: &Recipient) -> SmallVec<[Target; 2]> {
        match r {
            Recipient::Target(n) => ctx
                .targets
                .get(usize::from(*n))
                .map(|s| s.iter().copied().collect())
                .unwrap_or_default(),
            Recipient::Player(p) => self
                .player_ref(ctx, p)
                .map(Target::Player)
                .into_iter()
                .collect(),
            Recipient::Object(o) => self
                .object_refs(ctx, o)
                .into_iter()
                .map(Target::Object)
                .collect(),
        }
    }

    /// A non-negative amount (CR 107.1b: a negative result counts as 0 for counts).
    pub(crate) fn amount(&self, ctx: &EffectCtx, q: &Quantity) -> u32 {
        u32::try_from(self.quantity(ctx, q).max(0)).unwrap_or(u32::MAX)
    }

    pub(crate) fn quantity(&self, ctx: &EffectCtx, q: &Quantity) -> i64 {
        match q {
            Quantity::Fixed(n) => *n,
            Quantity::Expr(e) => match e {
                QuantityExpr::X => i64::from(ctx.x),
                QuantityExpr::PowerOf(o) => self
                    .object_ref(ctx, o)
                    .and_then(|id| self.known(ctx, id))
                    .map_or(0, |k| i64::from(k.power)),
                QuantityExpr::ManaValueOf(o) => self
                    .object_ref(ctx, o)
                    .and_then(|id| self.known(ctx, id))
                    .map_or(0, |k| i64::from(self.def(k.def).mana_value())),
                QuantityExpr::Count(f) => i64::try_from(
                    self.objects
                        .iter()
                        .filter(|(id, o)| {
                            zone_matches(f.zone.unwrap_or(ZoneName::Battlefield), o.zone)
                                && self.matches_filter(*id, f, ctx.controller, Some(ctx.source))
                        })
                        .count(),
                )
                .unwrap_or(0),
                QuantityExpr::Devotion(colors) => self.devotion(ctx.controller, colors),
                QuantityExpr::CardsInLibrary(p) => self.player_ref(ctx, p).map_or(0, |p| {
                    i64::try_from(self.player(p).library.len()).unwrap_or(0)
                }),
                QuantityExpr::MostDiscardedThisWay => ctx
                    .discarded
                    .iter()
                    .map(|(_, n)| i64::from(*n))
                    .max()
                    .unwrap_or(0),
                // The effect's own choices, then the source's "as this enters" choice.
                QuantityExpr::Var(name) => ctx
                    .vars
                    .iter()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| *v)
                    .or_else(|| self.object_var(ctx.source, name))
                    .unwrap_or(0),
                QuantityExpr::Plus(q, n) => self.quantity(ctx, q) + n,
                QuantityExpr::CountersOn { what, counter } => self
                    .object_ref(ctx, what)
                    .map_or(0, |id| i64::from(self.counters_on(id, *counter))),
                // Rejected by `support::check` until combat history is tracked (M4).
                QuantityExpr::TriggeringAmount => i64::from(ctx.triggering_amount.unwrap_or(0)),
                QuantityExpr::SpellsCastThisTurn { by, color } => i64::try_from(
                    self.cast_this_turn
                        .iter()
                        .filter(|r| self.player_rel(*by, ctx.controller, r.player))
                        .filter(|r| {
                            color.as_ref().is_none_or(|letters| {
                                letters
                                    .chars()
                                    .filter_map(Color::from_letter)
                                    .any(|c| r.colors.contains(c))
                            })
                        })
                        .count(),
                )
                .unwrap_or(0),
                QuantityExpr::HalfRoundedUp(q) => {
                    let n = self.quantity(ctx, q);
                    n / 2 + n % 2
                }
                QuantityExpr::LifeOf(p) => {
                    self.player_ref(ctx, p).map_or(0, |p| self.player(p).life)
                }
                QuantityExpr::TopOfLibraryManaValue(p) => self
                    .player_ref(ctx, p)
                    .and_then(|p| self.player(p).library.last().copied())
                    .map_or(0, |top| i64::from(self.mana_value_of(top))),
                QuantityExpr::MostSpellsCastByAnOpponent => self
                    .players
                    .iter()
                    .filter(|p| p.id != ctx.controller)
                    .map(|p| {
                        i64::try_from(
                            self.cast_this_turn
                                .iter()
                                .filter(|r| r.player == p.id)
                                .count(),
                        )
                        .unwrap_or(0)
                    })
                    .max()
                    .unwrap_or(0),
                QuantityExpr::CountOf { filter, controller } => {
                    let Some(p) = self.player_ref(ctx, controller) else {
                        return 0;
                    };
                    let mut f = filter.clone();
                    f.controller = mtg_cards::ir::PlayerRel::Any;
                    i64::try_from(
                        self.battlefield
                            .iter()
                            .filter(|id| {
                                self.obj(**id).is_some_and(|o| o.controller == p)
                                    && self.matches_filter(
                                        **id,
                                        &f,
                                        ctx.controller,
                                        Some(ctx.source),
                                    )
                            })
                            .count(),
                    )
                    .unwrap_or(0)
                }
                QuantityExpr::Neg(q) => -self.quantity(ctx, q),
                QuantityExpr::OpponentsDealtCombatDamageThisTurn => {
                    i64::from((self.combat_damaged & !(1u8 << ctx.controller.0)).count_ones())
                }
            },
        }
    }

    /// CR 700.5: mana symbols of the given colors among mana costs of permanents `p` controls.
    fn devotion(&self, p: PlayerId, colors: &str) -> i64 {
        let wanted: Vec<Color> = colors.chars().filter_map(Color::from_letter).collect();
        let mut n = 0;
        for &id in &self.battlefield {
            let Some(o) = self.obj(id) else { continue };
            if o.controller != p {
                continue;
            }
            if let Some(cost) = &self.def(o.def).mana_cost {
                n += cost
                    .symbols()
                    .iter()
                    .filter(|s| wanted.iter().any(|c| s.colors().contains(*c)))
                    .count();
            }
        }
        i64::try_from(n).unwrap_or(0)
    }

    pub(crate) fn condition(&self, ctx: &EffectCtx, c: &Condition) -> bool {
        match c {
            Condition::YourTurn => self.turn.active == ctx.controller,
            Condition::ThisTapped => self.obj(ctx.source).is_some_and(|o| o.tapped),
            Condition::InCombat => {
                use crate::state::Step;
                matches!(
                    self.turn.step,
                    Step::BeginCombat
                        | Step::DeclareAttackers
                        | Step::DeclareBlockers
                        | Step::FirstStrikeDamage
                        | Step::CombatDamage
                        | Step::EndCombat
                )
            }
            Condition::AtLeast(a, b) => self.quantity(ctx, a) >= self.quantity(ctx, b),
            // Recorded when the ability adds mana (see `mana_added`).
            Condition::Kicked => self.kicked.contains(&ctx.source),
            Condition::TargetMatches { target, filter } => ctx
                .targets
                .get(usize::from(*target))
                .and_then(|slot| slot.first())
                .is_some_and(|t| match t {
                    Target::Object(id) => self.matches_card(ctx, *id, filter),
                    Target::Player(_) | Target::StackAbility { .. } => false,
                }),
            Condition::UnusedThisTurn => !self
                .activated_this_turn
                .contains(&(ctx.source, ctx.ability.ability)),
            Condition::TopOfLibraryMatches { who, filter } => self
                .player_ref(ctx, who)
                .and_then(|p| self.player(p).library.last().copied())
                .is_some_and(|top| self.matches_card(ctx, top, filter)),
            Condition::Not(c) => !self.condition(ctx, c),
            // CR 903.3d: a commander on the battlefield the controller controls.
            Condition::ControlCommander => self.controls_commander(ctx.controller),
        }
    }

    /// Outcomes of a mana spec the game computes now (M6 plan C10), for the
    /// ability in `ctx`. Distinct bags, in WUBRG order; none if it makes no mana.
    pub(crate) fn dynamic_mana_bags(&self, spec: &ManaSpec, ctx: &EffectCtx) -> Vec<ManaBag> {
        let one_each = |cs: mtg_core::Colors| -> Vec<ManaBag> {
            cs.iter()
                .map(|c| {
                    let mut b = [0; 6];
                    b[ManaType::of_color(c).index()] = 1;
                    b
                })
                .collect()
        };
        let n = |q: &Quantity| u8::try_from(self.amount(ctx, q)).unwrap_or(u8::MAX);
        match spec {
            // CR 903.4f: no commander, no mana; a colorless identity, none either.
            ManaSpec::CommanderIdentity => self
                .commander_identity(ctx.controller)
                .map(one_each)
                .unwrap_or_default(),
            ManaSpec::ColorOf(r) => one_each(
                self.object_refs(ctx, r)
                    .iter()
                    .filter_map(|id| self.chars(*id).map(|c| c.colors()))
                    .fold(mtg_core::Colors::COLORLESS, mtg_core::Colors::union),
            ),
            ManaSpec::ColorsAmong(f) => one_each(
                self.battlefield
                    .iter()
                    .filter(|id| self.matches_filter(**id, f, ctx.controller, Some(ctx.source)))
                    .filter_map(|id| self.chars(*id).map(|c| c.colors()))
                    .fold(mtg_core::Colors::COLORLESS, mtg_core::Colors::union),
            ),
            ManaSpec::OpponentLandColors => one_each(self.opponent_land_colors(ctx.controller)),
            ManaSpec::ChosenColor(name) => self
                .object_var(ctx.source, name)
                .and_then(|i| Color::ALL.get(usize::try_from(i).ok()?).copied())
                .map(|c| one_each(mtg_core::Colors::single(c)))
                .unwrap_or_default(),
            ManaSpec::Times { mana, times } => {
                let times = n(times);
                let Some(unit) = crate::card::mana_output(&ManaSpec::Fixed(mana.clone()))
                    .ok()
                    .and_then(|alts| alts.into_iter().next())
                else {
                    return Vec::new();
                };
                let mut bag = [0u8; 6];
                for t in ManaType::ALL {
                    let per = unit.iter().filter(|u| **u == t.bit()).count();
                    bag[t.index()] = u8::try_from(per).unwrap_or(0).saturating_mul(times);
                }
                if bag.iter().all(|x| *x == 0) {
                    Vec::new()
                } else {
                    vec![bag]
                }
            }
            ManaSpec::AnyOneColor(q) => {
                let k = n(q);
                if k == 0 {
                    return Vec::new();
                }
                one_each(mtg_core::Colors::ALL)
                    .into_iter()
                    .map(|mut b| {
                        for x in &mut b {
                            *x = x.saturating_mul(k);
                        }
                        b
                    })
                    .collect()
            }
            ManaSpec::AnyColor(q) => {
                let k = usize::from(n(q));
                crate::card::expand_bags(&smallvec::smallvec![smallvec::SmallVec::from_elem(
                    crate::card::ANY_COLOR,
                    k
                )])
                .into_iter()
                .filter(|b| b.iter().any(|x| *x > 0))
                .collect()
            }
            ManaSpec::Colorless(q) => {
                let k = n(q);
                if k == 0 {
                    Vec::new()
                } else {
                    vec![[0, 0, 0, 0, 0, k]]
                }
            }
            other => mana_bags(other),
        }
    }

    /// The mana a mana ability's effect would make now: its `AddMana`, or the
    /// branch of an "if ..., instead" that applies.
    pub(crate) fn mana_effect_bags(&self, e: &Effect, ctx: &EffectCtx) -> Vec<ManaBag> {
        match e {
            Effect::AddMana { mana, .. } => {
                if crate::card::is_dynamic_spec(mana) {
                    self.dynamic_mana_bags(mana, ctx)
                } else {
                    mana_bags(mana)
                }
            }
            Effect::Seq(v) => v
                .iter()
                .flat_map(|x| self.mana_effect_bags(x, ctx))
                .collect(),
            Effect::If {
                condition,
                then,
                otherwise,
            } => {
                if self.condition(ctx, condition) {
                    self.mana_effect_bags(then, ctx)
                } else {
                    otherwise
                        .as_deref()
                        .map(|o| self.mana_effect_bags(o, ctx))
                        .unwrap_or_default()
                }
            }
            _ => Vec::new(),
        }
    }

    /// A choice made as `id` entered (CR 614.12).
    pub(crate) fn object_var(&self, id: ObjectId, name: &str) -> Option<i64> {
        self.object_vars
            .iter()
            .find(|(o, k, _)| *o == id && k == name)
            .map(|(_, _, v)| *v)
    }

    /// CR 106.7: the colors lands `p`'s opponents control could produce.
    /// Abilities of the same kind ("a land an opponent controls could
    /// produce") are ignored: they define no type of mana for each other.
    fn opponent_land_colors(&self, p: PlayerId) -> mtg_core::Colors {
        let mut out = mtg_core::Colors::COLORLESS;
        for &id in &self.battlefield {
            let Some(o) = self.obj(id) else { continue };
            let is_land = self
                .chars(id)
                .is_some_and(|c| c.type_line().has_type(mtg_core::CardType::Land));
            if o.controller == p || !self.in_game(o.controller) || !is_land {
                continue;
            }
            for (r, m) in self.mana_abilities_of(id) {
                let bags: Vec<ManaBag> = if m.dynamic {
                    let ctx = EffectCtx::new(r, id, o.controller);
                    let mut v = Vec::new();
                    if let Some(Ability::Activated(x)) = self.ability_at(r) {
                        x.effect.walk(&mut |e| {
                            if let Effect::AddMana { mana, .. } = e
                                && *mana != ManaSpec::OpponentLandColors
                            {
                                v.extend(self.dynamic_mana_bags(mana, &ctx));
                            }
                        });
                    }
                    v
                } else {
                    m.bags.to_vec()
                };
                for b in bags {
                    for c in Color::ALL {
                        if b[ManaType::of_color(c).index()] > 0 {
                            out.insert(c);
                        }
                    }
                }
            }
        }
        out
    }

    /// A card filter with names resolved from the context (`named` refers to a chosen name).
    fn matches_card(&self, ctx: &EffectCtx, id: ObjectId, f: &ObjectFilter) -> bool {
        if let Some(store) = &f.named {
            let chosen = ctx
                .names
                .iter()
                .find(|(k, _)| k == store)
                .and_then(|(_, d)| *d);
            let def = self.obj(id).map(|o| o.def);
            if chosen.is_none() || def != chosen {
                return false;
            }
        }
        let mut f2 = f.clone();
        f2.named = None;
        f2.zone = None;
        self.matches_filter(id, &f2, ctx.controller, Some(ctx.source))
    }

    // ---- object sets -------------------------------------------------------

    /// Objects a set denotes right now. `ChosenFrom` sets go through [`Game::pick_for`].
    fn set_now(&self, ctx: &EffectCtx, set: &ObjectSet) -> Vec<ObjectId> {
        match set {
            ObjectSet::Zone { owner, zone } => {
                self.player_ref(ctx, owner).map_or_else(Vec::new, |p| {
                    let pl = self.player(p);
                    match zone {
                        ZoneName::Graveyard => pl.graveyard.clone(),
                        ZoneName::Hand => pl.hand.clone(),
                        ZoneName::Library => pl.library.clone(),
                        _ => Vec::new(),
                    }
                })
            }
            ObjectSet::One(r) => self.object_refs(ctx, r).into_vec(),
            ObjectSet::All(f) => {
                let zone = f.zone.unwrap_or(ZoneName::Battlefield);
                self.objects_in_named_zone(zone)
                    .into_iter()
                    .filter(|id| self.matches_card(ctx, *id, f))
                    .collect()
            }
            ObjectSet::TopOfLibrary { owner, count } => match self.player_ref(ctx, owner) {
                Some(p) => self.top_cards(p, self.amount(ctx, count)),
                None => Vec::new(),
            },
            ObjectSet::ChosenFrom { .. } => Vec::new(),
        }
    }

    fn objects_in_named_zone(&self, z: ZoneName) -> Vec<ObjectId> {
        match z {
            ZoneName::Battlefield => self.battlefield.clone(),
            ZoneName::Exile => self.exile.clone(),
            ZoneName::Stack => self
                .stack
                .iter()
                .filter(|s| s.is_spell())
                .map(|s| s.object)
                .collect(),
            ZoneName::Command => Vec::new(),
            ZoneName::Hand | ZoneName::Library | ZoneName::Graveyard => self
                .players
                .iter()
                .flat_map(|p| match z {
                    ZoneName::Hand => p.hand.clone(),
                    ZoneName::Library => p.library.clone(),
                    _ => p.graveyard.clone(),
                })
                .collect(),
        }
    }

    /// For a `ChosenFrom` set: the pick that chooses its members.
    fn pick_for(&self, t: &EffectTask, set: &ObjectSet, purpose: CardPurpose) -> Option<Pick> {
        let ObjectSet::ChosenFrom {
            who,
            zone,
            filter,
            count,
        } = set
        else {
            return None;
        };
        let who = self.player_ref(&t.ctx, who)?;
        let candidates: Vec<ObjectId> = match zone {
            ZoneName::Hand => self.player(who).hand.clone(),
            ZoneName::Graveyard => self.player(who).graveyard.clone(),
            ZoneName::Library => self.player(who).library.clone(),
            ZoneName::Battlefield => self
                .battlefield
                .iter()
                .copied()
                .filter(|id| self.obj(*id).is_some_and(|o| o.controller == who))
                .collect(),
            ZoneName::Exile | ZoneName::Stack | ZoneName::Command => Vec::new(),
        }
        .into_iter()
        .filter(|id| self.matches_card(&t.ctx, *id, filter))
        .collect();
        let count = self.amount(&t.ctx, count);
        Some(exact_pick(
            who,
            purpose,
            candidates,
            count,
            AfterPick::Discard,
        ))
    }

    fn own_permanents(&self, ctx: &EffectCtx, who: PlayerId, f: &ObjectFilter) -> Vec<ObjectId> {
        self.battlefield
            .iter()
            .copied()
            .filter(|id| {
                self.obj(*id).is_some_and(|o| o.controller == who) && self.matches_card(ctx, *id, f)
            })
            .collect()
    }

    fn on_battlefield_now(&self, id: ObjectId) -> bool {
        // A phased-out permanent is treated as though it doesn't exist (CR 702.26b).
        self.obj(id).is_some_and(|o| o.zone == ZoneId::Battlefield)
            && self.battlefield.contains(&id)
    }

    /// The top `n` cards of `p`'s library, top first.
    fn top_cards(&self, p: PlayerId, n: u32) -> Vec<ObjectId> {
        self.player(p)
            .library
            .iter()
            .rev()
            .take(n as usize)
            .copied()
            .collect()
    }

    // ---- primitive actions ------------------------------------------------

    /// Moves an object, recording last known information first (CR 608.2h).
    fn fx_move(
        &mut self,
        ctx: &mut EffectCtx,
        id: ObjectId,
        to: ZoneId,
        top: bool,
    ) -> Option<ObjectId> {
        if let Some(k) = self.known(ctx, id) {
            ctx.lki.push(k);
        }
        let new = self.move_object(id, to, top);
        follow_source(ctx, id, new, to);
        new
    }

    /// The zone a destination means for `id` (its owner's zones).
    fn destination_zone(&self, id: ObjectId, to: Destination) -> Option<ZoneId> {
        let owner = self.obj(id)?.owner;
        Some(match to {
            Destination::TopOfLibrary | Destination::BottomOfLibrary => ZoneId::Library(owner),
            Destination::Hand => ZoneId::Hand(owner),
            Destination::Graveyard => ZoneId::Graveyard(owner),
            Destination::Exile => ZoneId::Exile,
            Destination::Battlefield | Destination::BattlefieldTapped => ZoneId::Battlefield,
        })
    }

    /// [`Game::place`], unless a commander's owner must first answer
    /// CR 903.9b: then a `Place` frame asks and moves it after this step.
    fn place_or_defer(&mut self, t: &mut EffectTask, id: ObjectId, to: Destination, who: PlayerId) {
        let needs = self
            .destination_zone(id, to)
            .is_some_and(|z| self.commander_choice_needed(id, z).is_some())
            || (matches!(
                to,
                Destination::Battlefield | Destination::BattlefieldTapped
            ) && self.entering_question(id, who).is_some());
        if needs {
            t.frames.push(Frame::Place {
                ids: vec![id],
                to,
                who,
            });
        } else {
            self.place(&mut t.ctx, id, to, who);
        }
    }

    /// Puts an object into a destination. Cards already in their owner's
    /// library are reordered in place (moving within a zone isn't a zone change).
    fn place(&mut self, ctx: &mut EffectCtx, id: ObjectId, to: Destination, who: PlayerId) {
        let Some(o) = self.obj(id).copied() else {
            return;
        };
        // CR 702.26b: a phased-out permanent is treated as though it doesn't exist.
        if o.zone == ZoneId::Battlefield && !self.on_battlefield_now(id) {
            return;
        }
        let owner = o.owner;
        match to {
            Destination::TopOfLibrary | Destination::BottomOfLibrary => {
                let top = to == Destination::TopOfLibrary;
                if o.zone == ZoneId::Library(owner) {
                    let lib = &mut self.player_mut(owner).library;
                    lib.retain(|x| *x != id);
                    if top { lib.push(id) } else { lib.insert(0, id) }
                } else {
                    self.fx_move(ctx, id, ZoneId::Library(owner), top);
                }
            }
            Destination::Hand => {
                self.fx_move(ctx, id, ZoneId::Hand(owner), true);
            }
            Destination::Graveyard => {
                self.fx_move(ctx, id, ZoneId::Graveyard(owner), true);
            }
            Destination::Exile => {
                self.fx_move(ctx, id, ZoneId::Exile, true);
            }
            Destination::Battlefield | Destination::BattlefieldTapped => {
                // Grafdigger's Cage: it stays where it is.
                if self.entering_prohibited(id) {
                    return;
                }
                // CR 614.1a: "if this would enter, … instead" (R4: it runs after this effect).
                let applied = ctx.applied.clone();
                if self.replace_entering(id, who, &applied) {
                    return;
                }
                if let Some(k) = self.known(ctx, id) {
                    ctx.lki.push(k);
                }
                let tapped = to == Destination::BattlefieldTapped;
                // CR 110.2: it enters under the control of the player putting
                // it there; CR 110.5b: tapped if the effect says so.
                let new = self.move_object_with(id, ZoneId::Battlefield, true, |g, new| {
                    if let Some(n) = g.objects.get_mut(new) {
                        n.controller = who;
                        n.tapped |= tapped;
                    }
                    g.bump_layers();
                });
                follow_source(ctx, id, new, ZoneId::Battlefield);
            }
        }
    }

    /// CR 701.8a; indestructible permanents aren't destroyed (CR 702.12b).
    fn destroy(&mut self, ctx: &mut EffectCtx, id: ObjectId) {
        let Some(o) = self.obj(id).copied() else {
            return;
        };
        if o.zone != ZoneId::Battlefield
            || self.keywords(id).has(crate::card::Keywords::INDESTRUCTIBLE)
        {
            return;
        }
        self.log(Event::Destroyed { object: id });
        self.fx_move(ctx, id, ZoneId::Graveyard(o.owner), true);
    }

    fn exile_all(
        &mut self,
        ctx: &mut EffectCtx,
        ids: &[ObjectId],
        remember: Option<String>,
        face_down: bool,
    ) {
        let mut new_ids = SmallVec::new();
        for &id in ids {
            if self.obj(id).is_some_and(|o| o.zone != ZoneId::Exile)
                && let Some(new) = self.fx_move(ctx, id, ZoneId::Exile, true)
            {
                if face_down {
                    self.face_down.push(new);
                }
                new_ids.push(new);
            }
        }
        if let Some(name) = remember {
            // CR 607.2a: a permanent's ability that exiles links the cards to
            // the permanent's other abilities.
            if self.on_battlefield(ctx.source) {
                let source = ctx.source;
                self.linked
                    .retain(|(s, k, _)| !(*s == source && *k == name));
                self.linked.push((source, name.clone(), new_ids.clone()));
            }
            ctx.remembered.retain(|(k, _)| *k != name);
            ctx.remembered.push((name, new_ids));
        }
    }

    fn sacrifice_all(&mut self, ctx: &mut EffectCtx, ids: &[ObjectId]) {
        self.begin_leaving(ids);
        for &id in ids {
            let Some(o) = self.obj(id).copied() else {
                continue;
            };
            if o.zone != ZoneId::Battlefield {
                continue;
            }
            self.log(Event::Sacrificed { object: id });
            self.fx_move(ctx, id, ZoneId::Graveyard(o.owner), true);
        }
        self.end_leaving();
    }

    /// CR 701.6a: a countered spell goes to its owner's graveyard.
    fn counter(&mut self, ctx: &mut EffectCtx, id: ObjectId) {
        self.counter_to(ctx, id, false);
    }

    /// Counters spell `id`, into exile if `exile` (Force of Negation).
    fn counter_to(&mut self, ctx: &mut EffectCtx, id: ObjectId, exile: bool) {
        let Some(o) = self.obj(id).copied() else {
            return;
        };
        // CR 101.2: "can't be countered" takes precedence.
        if o.zone != ZoneId::Stack
            || self.def(o.def).cant_be_countered()
            || self.uncounterable.contains(&id)
            || self.static_uncounterable(id)
            || self.has_player_rule(
                o.controller,
                mtg_cards::ir::PlayerRuleKind::SpellsCantBeCountered,
            )
        {
            return;
        }
        self.log(Event::Countered { object: id });
        let to = if exile {
            ZoneId::Exile
        } else {
            ZoneId::Graveyard(o.owner)
        };
        self.fx_move(ctx, id, to, true);
    }

    /// CR 701.6a: an ability is countered by removing it from the stack (the
    /// most recent one with this source and ability index).
    fn counter_ability(&mut self, source: ObjectId, ability: u8) {
        if let Some(pos) = self
            .stack
            .iter()
            .rposition(|s| s.object == source && s.ability == Some(ability))
        {
            self.stack.remove(pos);
            self.log(Event::Countered { object: source });
        }
    }

    fn discard(&mut self, t: &mut EffectTask, p: PlayerId, what: &DiscardSpec) {
        match what {
            DiscardSpec::Hand => {
                let hand = self.player(p).hand.clone();
                self.discard_all(&mut t.ctx, p, &hand);
            }
            DiscardSpec::Chosen { count, filter } => {
                let n = self.amount(&t.ctx, count);
                let candidates: Vec<ObjectId> = self
                    .player(p)
                    .hand
                    .clone()
                    .into_iter()
                    .filter(|id| self.matches_card(&t.ctx, *id, filter))
                    .collect();
                t.frames.push(Frame::Pick(exact_pick(
                    p,
                    CardPurpose::Discard,
                    candidates,
                    n,
                    AfterPick::Discard,
                )));
            }
            DiscardSpec::Random(q) => {
                let n = self.amount(&t.ctx, q);
                let mut hand = self.player(p).hand.clone();
                let mut chosen = Vec::new();
                for _ in 0..n {
                    if hand.is_empty() {
                        break;
                    }
                    let i = self.rng.index(hand.len());
                    chosen.push(hand.remove(i));
                }
                self.discard_all(&mut t.ctx, p, &chosen);
            }
        }
    }

    fn discard_all(&mut self, ctx: &mut EffectCtx, p: PlayerId, ids: &[ObjectId]) {
        let mut n = 0;
        let mut discarded: SmallVec<[ObjectId; 2]> = SmallVec::new();
        for &id in ids {
            if let Some(new) = self.fx_move(ctx, id, ZoneId::Graveyard(p), true) {
                n += 1;
                discarded.push(new);
                self.log(Event::Discarded {
                    player: p,
                    object: new,
                });
            }
        }
        match ctx.discarded.iter_mut().find(|(q, _)| *q == p) {
            Some((_, c)) => *c += n,
            None => ctx.discarded.push((p, n)),
        }
        // "the discarded card" later in the effect (Malcolm).
        ctx.remembered.retain(|(k, _)| k != "discarded");
        ctx.remembered.push(("discarded".to_owned(), discarded));
    }

    // ---- escapes (escapes.rs) ----------------------------------------------

    /// Doomsday: "Search your library and graveyard for five cards and exile
    /// the rest. Put the chosen cards on top of your library in any order."
    pub(crate) fn doomsday(&mut self, t: &mut EffectTask) {
        let who = t.ctx.controller;
        let pl = self.player(who);
        let pool: Vec<ObjectId> = pl.library.iter().chain(&pl.graveyard).copied().collect();
        t.frames.push(Frame::Pick(exact_pick(
            who,
            CardPurpose::Search,
            pool.clone(),
            5,
            AfterPick::Doomsday { pool },
        )));
    }

    /// Sylvan Library (after the two extra draws): choose two cards in hand
    /// drawn this turn; for each, pay 4 life or put it on top of the library.
    pub(crate) fn sylvan_library(&mut self, t: &mut EffectTask) {
        let who = t.ctx.controller;
        let drawn: Vec<ObjectId> = self
            .drawn_this_turn
            .iter()
            .filter(|(p, id)| {
                *p == who && self.obj(*id).is_some_and(|o| o.zone == ZoneId::Hand(who))
            })
            .map(|(_, id)| *id)
            .collect();
        t.frames.push(Frame::Pick(exact_pick(
            who,
            CardPurpose::Take,
            drawn,
            2,
            AfterPick::Sylvan,
        )));
    }

    /// Tainted Pact: exile the top card; you may put it into your hand
    /// unless its name matches another card exiled this way; repeat until a
    /// card is taken or two exiled cards share a name.
    pub(crate) fn tainted_pact(&mut self, t: &mut EffectTask) {
        t.frames.push(Frame::TaintedPact {
            names: Vec::new(),
            last: None,
        });
    }

    fn tainted_pact_step(
        &mut self,
        t: &mut EffectTask,
        mut names: Vec<DefId>,
        last: Option<ObjectId>,
    ) -> Flow {
        debug_assert!(
            last.is_none(),
            "an asked Tainted Pact frame is answered, not run"
        );
        let who = t.ctx.controller;
        let Some(&top) = self.player(who).library.last() else {
            return Flow::Continue;
        };
        let Some(new) = self.fx_move(&mut t.ctx, top, ZoneId::Exile, true) else {
            return Flow::Continue;
        };
        let Some(name) = self.obj(new).map(|o| o.def) else {
            return Flow::Continue;
        };
        // Two cards with the same name: the process stops.
        if names
            .iter()
            .any(|d| self.def(*d).name == self.def(name).name)
        {
            return Flow::Continue;
        }
        names.push(name);
        t.frames.push(Frame::TaintedPact {
            names,
            last: Some(new),
        });
        Flow::Decide(
            who,
            DecisionKind::Optional {
                source: new,
                kind: OptionalKind::May,
            },
            "608.2d",
        )
    }

    pub(crate) fn shuffle_library(&mut self, p: PlayerId) {
        let mut lib = std::mem::take(&mut self.player_mut(p).library);
        self.rng.shuffle(&mut lib);
        self.player_mut(p).library = lib;
        self.log(Event::Shuffled { player: p });
    }

    pub(crate) fn untap(&mut self, id: ObjectId) {
        if let Some(o) = self.objects.get_mut(id)
            && o.tapped
        {
            o.tapped = false;
            self.log(Event::Untapped { object: id });
        }
    }

    /// Draws `n` cards for `p`, handing a draw to the task stack when a
    /// replacement effect would change it (M4 plan R2).
    fn draws(&mut self, t: &mut EffectTask, p: PlayerId, n: u32) -> Flow {
        for i in 0..n {
            if !self.draw_replacements(p, &t.ctx.applied).is_empty()
                || self.draw_question(p).is_some()
            {
                let left = n - i - 1;
                if left > 0 {
                    t.frames.push(Frame::Draws { who: p, left });
                }
                return Flow::Yield(Box::new(Task::Draw(crate::replacement::DrawTask {
                    player: p,
                    applied: t.ctx.applied.clone(),
                })));
            }
            self.draw(p);
        }
        Flow::Continue
    }

    /// CR 707.12: a copy of a card, in the same zone, owned and controlled by
    /// `controller`; it ceases to exist outside the stack and battlefield
    /// (CR 704.5e).
    fn copy_card(&mut self, id: ObjectId, controller: PlayerId) -> Option<ObjectId> {
        let o = *self.obj(id)?;
        let timestamp = self.next_timestamp();
        let copy = self.objects.alloc(crate::state::GameObject {
            def: o.def,
            owner: controller,
            controller,
            zone: o.zone,
            tapped: false,
            damage: 0,
            deathtouch_damage: false,
            control_since: self.turn.number,
            timestamp,
        });
        if o.zone != ZoneId::Stack {
            self.zone_vec_mut(o.zone).push(copy);
        }
        self.copies.push(copy);
        self.log(Event::CopyCreated {
            object: copy,
            of: id,
        });
        Some(copy)
    }

    /// The copiable values of `id` plus a copy effect's exceptions (CR 707.9);
    /// granted abilities come from the granting definition.
    /// Whether copiable values `cv` include the card type named `word`.
    fn copy_has_type(&self, cv: &crate::layers::CopyValues, word: &str) -> bool {
        mtg_core::CardType::ALL
            .iter()
            .find(|t| t.word().eq_ignore_ascii_case(word))
            .is_some_and(|t| cv.types.contains(t) || self.def(cv.def).is(*t))
    }

    fn copy_values_with(
        &self,
        id: ObjectId,
        except: &mtg_cards::ir::CopyExcept,
        granter: DefId,
    ) -> Option<crate::layers::CopyValues> {
        let mut cv = self.copiable_values(id)?;
        for w in &except.types {
            if let Some(t) = mtg_core::CardType::ALL
                .iter()
                .find(|t| t.word().eq_ignore_ascii_case(w))
                && !cv.types.contains(t)
            {
                cv.types.push(*t);
            }
        }
        for w in &except.subtypes {
            let st = mtg_core::Subtype(w.clone());
            if !cv.subtypes.contains(&st) {
                cv.subtypes.push(st);
            }
        }
        let def = self.def(granter);
        for a in &except.abilities {
            if let Some(ability) = def.granted_index(a) {
                cv.abilities.push(AbilityRef {
                    def: granter,
                    ability,
                });
            }
        }
        cv.not_legendary |= except.not_legendary;
        Some(cv)
    }

    /// CR 111.10, 707.2: a token that's a copy, under `controller`'s control.
    fn create_token_copy(
        &mut self,
        cv: crate::layers::CopyValues,
        controller: PlayerId,
    ) -> Option<ObjectId> {
        let def = self.pool.copy_token()?;
        self.bump_layers();
        let timestamp = self.next_timestamp();
        let id = self.objects.alloc(crate::state::GameObject {
            def,
            owner: controller,
            controller,
            zone: ZoneId::Battlefield,
            tapped: false,
            damage: 0,
            deathtouch_damage: false,
            control_since: self.turn.number,
            timestamp,
        });
        self.battlefield.push(id);
        // The copy effect exists as the token is created, so its "enters"
        // abilities trigger (CR 603.6a).
        self.add_copy_effect(id, cv);
        self.log(Event::TokenCreated {
            player: controller,
            object: id,
            def,
        });
        Some(id)
    }

    /// CR 707.10: a copy of a spell, put on the stack above it (not cast),
    /// controlled by `controller`, with the same modes, targets and X.
    fn copy_spell(&mut self, spell: ObjectId, controller: PlayerId) -> Option<ObjectId> {
        let o = *self.obj(spell)?;
        if o.zone != ZoneId::Stack {
            return None;
        }
        let item = self
            .stack
            .iter()
            .rev()
            .find(|s| s.is_spell() && s.object == spell)?
            .clone();
        let timestamp = self.next_timestamp();
        let copy = self.objects.alloc(crate::state::GameObject {
            def: o.def,
            owner: controller,
            controller,
            zone: ZoneId::Stack,
            tapped: false,
            damage: 0,
            deathtouch_damage: false,
            control_since: self.turn.number,
            timestamp,
        });
        self.copies.push(copy);
        self.stack.push(crate::state::StackItem {
            object: copy,
            controller,
            trigger: None,
            delayed: None,
            ..item
        });
        self.bump_layers();
        self.log(Event::CopyCreated {
            object: copy,
            of: spell,
        });
        Some(copy)
    }

    /// CR 707.10: copies the highest instance of `source`'s activated ability
    /// `ability` on the stack, for `controller` (Rings of Brighthearth). The
    /// one that triggered is the highest: nothing else can be activated
    /// before its trigger goes on the stack.
    fn copy_ability(
        &mut self,
        source: ObjectId,
        ability: u8,
        controller: PlayerId,
    ) -> Option<Target> {
        let same = |s: &crate::state::StackItem| s.object == source && s.ability == Some(ability);
        let item = self
            .stack
            .iter()
            .rev()
            .find(|s| same(s) && s.trigger.is_none() && s.delayed.is_none())?
            .clone();
        self.stack
            .push(crate::state::StackItem { controller, ..item });
        self.log(Event::AbilityCopied {
            source,
            ability,
            controller,
        });
        let nth = self
            .stack
            .iter()
            .filter(|s| same(s))
            .count()
            .saturating_sub(1);
        Some(Target::StackAbility {
            source,
            ability,
            nth: u8::try_from(nth).unwrap_or(u8::MAX),
        })
    }

    /// CR 707.10c: for each target of a copied spell, keep it or choose a
    /// new legal one.
    fn change_targets_step(
        &mut self,
        t: &mut EffectTask,
        target: Target,
        index: u8,
        chooser: PlayerId,
    ) -> Flow {
        let Some(item) = self
            .stack_position(target)
            .map(|pos| self.stack[pos].clone())
        else {
            return Flow::Continue;
        };
        let Some(&current) = item.targets.get(usize::from(index)) else {
            return Flow::Continue;
        };
        let def = self.def(item.source_def);
        let slot = usize::from(item.slots[usize::from(index)]);
        // Its target requirements, from its controller's point of view.
        let spec = match item.ability.map(|i| def.abilities.get(usize::from(i))) {
            None => def.spell().and_then(|s| s.targets.get(slot)),
            Some(Some(Ability::Activated(x))) => x.targets.get(slot),
            Some(Some(Ability::Triggered(tr))) => tr.targets.get(slot),
            Some(_) => None,
        }
        .map(|s| s.what.clone());
        let options: Vec<Target> = spec
            .map(|what| {
                self.legal_targets(&what, item.controller, Some(item.object), Some(def))
                    .into_iter()
                    .filter(|x| *x != current && *x != target)
                    .collect()
            })
            .unwrap_or_default();
        t.frames.push(Frame::ChangeTargets {
            item: target,
            index: index + 1,
            chooser,
        });
        if options.is_empty() {
            return Flow::Continue;
        }
        t.frames.push(Frame::ChangeTargets {
            item: target,
            index,
            chooser,
        });
        Flow::Decide(
            chooser,
            DecisionKind::ChangeTarget {
                spell: item.object,
                index,
                current,
                options,
            },
            "115.7d",
        )
    }

    /// The mana requirement of a mana cost part.
    pub(crate) fn part_req(&self, ctx: &EffectCtx, part: &CostPart) -> CostReq {
        match part {
            CostPart::Mana(s) => mana_req(s),
            CostPart::ManaRepeated { mana, times } => {
                let one = mana_req(mana);
                let n = self.amount(ctx, times);
                CostReq {
                    colored: one.colored.map(|c| c * n),
                    colorless: one.colorless * n,
                    generic: one.generic * n,
                }
            }
            _ => CostReq::default(),
        }
    }

    /// Adds mana from an effect; a mana ability that taps its source reports
    /// the permanent as "tapped for mana" (Kinnan, CR 605.1b).
    fn mana_added(&mut self, ctx: &EffectCtx, who: PlayerId, bag: &ManaBag) {
        self.add_mana_bag(who, bag, ctx.source);
        // "if you haven't added mana with this ability this turn": now it has.
        if bag.iter().any(|n| *n > 0)
            && matches!(
                self.def(ctx.ability.def).abilities.get(usize::from(ctx.ability.ability)),
                Some(Ability::Triggered(t)) if t.condition == Some(Condition::UnusedThisTurn)
            )
        {
            self.activated_this_turn
                .push((ctx.source, ctx.ability.ability));
        }
        // CR 106.6: "Spend this mana only to ..." marks the units it made.
        let mut restricted = None;
        if let Some(e) = root(&self.pool, ctx.ability) {
            e.walk(&mut |x| {
                if let Effect::AddMana {
                    spend_only_on: Some(_),
                    uncounterable,
                    ..
                } = x
                {
                    restricted = Some(*uncounterable);
                }
            });
        }
        if let Some(uncounterable) = restricted {
            for t in ManaType::ALL {
                for _ in 0..bag[t.index()] {
                    self.restricted.push(crate::state::RestrictedMana {
                        player: who,
                        mana: t,
                        ability: ctx.ability,
                        uncounterable,
                    });
                }
            }
        }
        if ctx.tapped_for_mana && self.pool.has_triggers() {
            self.log(Event::TappedForMana {
                player: who,
                source: ctx.source,
                produced: *bag,
            });
        }
    }

    /// CR 701.47a: N +1/+1 counters on the Army; if it isn't the subtype, it
    /// becomes that subtype in addition (layer 4, for as long as it exists).
    fn amass_onto(&mut self, army: ObjectId, n: u32, subtype: &str) {
        self.add_counters(army, mtg_cards::ir::CounterKind::PlusOne, n);
        let has = self
            .chars(army)
            .is_some_and(|c| c.type_line().has_subtype(subtype));
        if !has {
            self.add_effect(
                SmallVec::from_elem(army, 1),
                crate::layers::LayerEffect::AddSubtype(subtype.to_owned()),
                crate::layers::Duration::Permanent,
            );
        }
    }

    /// CR 701.47a: amass.
    fn amass(&mut self, t: &mut EffectTask, subtype: &str, amount: &Quantity) -> Flow {
        let who = t.ctx.controller;
        let n = self.amount(&t.ctx, amount);
        let army = ObjectFilter {
            types: vec!["creature".into()],
            subtypes: vec!["Army".into()],
            ..ObjectFilter::default()
        };
        let mut armies = self.own_permanents(&t.ctx, who, &army);
        if armies.is_empty() {
            if let Some(def) = self.pool.token(&crate::card::amass_token(subtype)) {
                self.create_tokens(def, who, 1);
            }
            armies = self.own_permanents(&t.ctx, who, &army);
        }
        match armies.as_slice() {
            [] => {}
            [one] => self.amass_onto(*one, n, subtype),
            _ => t.frames.push(Frame::Pick(exact_pick(
                who,
                CardPurpose::PutCounters,
                armies,
                1,
                AfterPick::Amass {
                    n,
                    subtype: subtype.to_owned(),
                },
            ))),
        }
        Flow::Continue
    }

    pub(crate) fn add_mana_bag(&mut self, p: PlayerId, bag: &ManaBag, source: ObjectId) {
        for t in ManaType::ALL {
            for _ in 0..bag[t.index()] {
                self.player_mut(p).pool.add(t, 1);
                self.log(Event::ManaAdded {
                    player: p,
                    mana: t,
                    source,
                });
            }
        }
    }

    /// Reveal from the top until a match (Demonic Consultation's shape).
    fn reveal_until(
        &mut self,
        t: &mut EffectTask,
        p: PlayerId,
        filter: &ObjectFilter,
        found_to: Destination,
        rest_to: Destination,
    ) {
        let mut revealed = Vec::new();
        let mut found = None;
        for id in self.player(p).library.iter().rev().copied() {
            revealed.push(id);
            if self.matches_card(&t.ctx, id, filter) {
                found = Some(id);
                break;
            }
        }
        self.log(Event::Revealed {
            player: p,
            objects: revealed.clone(),
        });
        let mut rest: Vec<ObjectId> = revealed
            .into_iter()
            .filter(|id| Some(*id) != found)
            .collect();
        if let Some(f) = found {
            self.place_or_defer(t, f, found_to, p);
        }
        if rest_to == Destination::BottomOfLibrary {
            // The rest go to the bottom in a random order.
            self.rng.shuffle(&mut rest);
        }
        for id in rest {
            self.place_or_defer(t, id, rest_to, p);
        }
    }

    fn win_by_effect(&mut self, p: PlayerId) {
        // CR 104.2b: an effect states that a player wins; the game ends,
        // unless something says that player can't win.
        if !self.can_win(p) {
            return;
        }
        self.log(Event::WonByEffect { player: p });
        self.player_mut(p).status = crate::state::PlayerStatus::Won;
        let turn = self.turn.number;
        self.end_game(crate::engine::GameResult::Win { player: p, turn });
    }

    // ---- picks ---------------------------------------------------------------

    fn pick_step(&mut self, t: &mut EffectTask, mut p: Pick) -> Flow {
        p.candidates.retain(|id| self.obj(*id).is_some());
        let left = u32::try_from(p.candidates.len()).unwrap_or(u32::MAX);
        p.max = p.max.min(left);
        p.min = p.min.min(p.max);
        if p.max == 0 {
            self.finish_pick(t, p);
            return Flow::Continue;
        }
        let ordered = matches!(p.then, AfterPick::Place(_) | AfterPick::ScryBottom);
        // No choice exists when every remaining candidate must be taken and
        // their order doesn't matter, or only one card is left to place.
        if p.min == left && (!ordered || left == 1) {
            let all = std::mem::take(&mut p.candidates);
            for id in all {
                match &p.then {
                    AfterPick::Place(to) => {
                        let to = *to;
                        self.place_or_defer(t, id, to, p.who);
                    }
                    _ => p.picked.push(id),
                }
            }
            p.max = 0;
            p.min = 0;
            self.finish_pick(t, p);
            return Flow::Continue;
        }
        let mut cards: Vec<Option<ObjectId>> = Vec::with_capacity(p.candidates.len() + 1);
        if p.min == 0 {
            cards.push(None);
        }
        cards.extend(p.candidates.iter().copied().map(Some));
        let who = p.who;
        let purpose = p.purpose;
        let cr = match purpose {
            CardPurpose::Search => "701.23a",
            CardPurpose::Discard => "701.9a",
            CardPurpose::Sacrifice => "701.21a",
            CardPurpose::PutOnBottom if matches!(p.then, AfterPick::ScryBottom) => "701.22a",
            _ => "608.2d",
        };
        let source = t.ctx.source;
        t.frames.push(Frame::Pick(p));
        Flow::Decide(
            who,
            DecisionKind::ChooseCard {
                source,
                purpose,
                cards,
            },
            cr,
        )
    }

    fn finish_pick(&mut self, t: &mut EffectTask, p: Pick) {
        let Pick {
            who,
            candidates,
            picked,
            then,
            ..
        } = p;
        match then {
            AfterPick::Place(_) => {}
            AfterPick::ScryBottom => {
                // The rest go on top in any order (CR 701.22a).
                let n = u32::try_from(candidates.len()).unwrap_or(0);
                t.frames.push(Frame::Pick(Pick {
                    who,
                    purpose: CardPurpose::PutOnTop,
                    candidates,
                    picked: Vec::new(),
                    max: n,
                    min: n,
                    then: AfterPick::Place(Destination::TopOfLibrary),
                }));
            }
            AfterPick::Discard => self.discard_all(&mut t.ctx, who, &picked),
            AfterPick::Doomsday { pool } => {
                let rest: Vec<ObjectId> =
                    pool.into_iter().filter(|id| !picked.contains(id)).collect();
                // The rest are exiled now; then the picks are put on top in
                // the order chosen (the pushed frame runs next).
                let n = u32::try_from(picked.len()).unwrap_or(0);
                t.frames.push(Frame::Pick(Pick {
                    who,
                    purpose: CardPurpose::PutOnTop,
                    candidates: picked,
                    picked: Vec::new(),
                    max: n,
                    min: n,
                    then: AfterPick::Place(Destination::TopOfLibrary),
                }));
                self.exile_all(&mut t.ctx, &rest, None, false);
            }
            AfterPick::Sylvan => {
                for card in picked.into_iter().rev() {
                    t.frames.push(Frame::SylvanCard { card });
                }
            }
            AfterPick::Sacrifice => self.sacrifice_all(&mut t.ctx, &picked),
            AfterPick::Amass { n, subtype } => {
                for id in picked {
                    self.amass_onto(id, n, &subtype);
                }
            }
            AfterPick::PutCounters(n) => {
                for id in picked {
                    self.add_counters(id, mtg_cards::ir::CounterKind::PlusOne, n);
                }
            }
            AfterPick::EnterAsCopy { node: at } => {
                let pool = Arc::clone(&self.pool);
                let except = match node(&pool, t.ctx.ability, &at) {
                    Some(Effect::EnterAsCopy { except, .. }) => except.clone(),
                    _ => mtg_cards::ir::CopyExcept::default(),
                };
                let granter = t.ctx.ability.def;
                let cv = picked
                    .first()
                    .and_then(|id| self.copy_values_with(*id, &except, granter));
                // Spark Double: counters by the copy's card type (CR 707.9, 614.1c).
                let extra: Vec<mtg_cards::ir::CounterKind> = cv
                    .as_ref()
                    .map(|cv| {
                        except
                            .counters
                            .iter()
                            .filter(|c| self.copy_has_type(cv, &c.if_type))
                            .map(|c| c.counter)
                            .collect()
                    })
                    .unwrap_or_default();
                let this = t.ctx.source;
                let controller = t.ctx.controller;
                self.move_object_with(this, ZoneId::Battlefield, true, |g, new| {
                    if let Some(o) = g.objects.get_mut(new) {
                        o.controller = controller;
                    }
                    if let Some(cv) = cv {
                        g.add_copy_effect(new, cv);
                    }
                    for k in extra {
                        g.add_counters(new, k, 1);
                    }
                });
            }
            AfterPick::Exile {
                remember,
                face_down,
            } => self.exile_all(&mut t.ctx, &picked, remember, face_down),
            AfterPick::Search {
                searcher,
                agent,
                to,
                reveal,
            } => {
                let who = searcher;
                self.log(Event::Searched {
                    player: who,
                    found: u32::try_from(picked.len()).unwrap_or(0),
                });
                // Opposition Agent: the searcher exiles each card found; the
                // Agent's controller may play them (linked, CR 607.2a).
                if let Some(agent) = agent {
                    let mut exiled: SmallVec<[ObjectId; 2]> = SmallVec::new();
                    for id in picked {
                        if let Some(new) = self.fx_move(&mut t.ctx, id, ZoneId::Exile, true) {
                            exiled.push(new);
                        }
                    }
                    match self
                        .linked
                        .iter_mut()
                        .find(|(s, k, _)| *s == agent && k == "found")
                    {
                        Some((_, _, v)) => v.extend(exiled),
                        None => self.linked.push((agent, "found".into(), exiled)),
                    }
                    return;
                }
                if reveal && !picked.is_empty() {
                    self.log(Event::Revealed {
                        player: who,
                        objects: picked.clone(),
                    });
                }
                if to == Destination::TopOfLibrary {
                    // CR 701.24b: search, shuffle, then put the found cards on top.
                    for &id in &picked {
                        self.player_mut(who).library.retain(|x| *x != id);
                    }
                    self.shuffle_library(who);
                    for &id in &picked {
                        self.player_mut(who).library.push(id);
                    }
                } else {
                    for id in picked {
                        self.place_or_defer(t, id, to, who);
                    }
                }
            }
            AfterPick::Take { to, rest, order } => {
                let rest: Vec<ObjectId> =
                    rest.into_iter().filter(|id| !picked.contains(id)).collect();
                if to == Destination::TopOfLibrary {
                    // The rest are dealt with first (possibly through ordering
                    // decisions), so the taken cards end up on top.
                    t.frames.push(Frame::Place {
                        ids: picked,
                        to,
                        who,
                    });
                    self.order_rest(t, who, rest, order);
                } else {
                    for id in picked {
                        self.place_or_defer(t, id, to, who);
                    }
                    self.order_rest(t, who, rest, order);
                }
            }
        }
    }

    /// Puts looked-at cards back per `order`.
    fn order_rest(
        &mut self,
        t: &mut EffectTask,
        who: PlayerId,
        rest: Vec<ObjectId>,
        order: RestOrder,
    ) {
        let n = u32::try_from(rest.len()).unwrap_or(0);
        match order {
            RestOrder::TopAnyOrder | RestOrder::BottomAnyOrder => {
                let (purpose, to) = if order == RestOrder::TopAnyOrder {
                    (CardPurpose::PutOnTop, Destination::TopOfLibrary)
                } else {
                    (CardPurpose::PutOnBottom, Destination::BottomOfLibrary)
                };
                t.frames.push(Frame::Pick(Pick {
                    who,
                    purpose,
                    candidates: rest,
                    picked: Vec::new(),
                    max: n,
                    min: n,
                    then: AfterPick::Place(to),
                }));
            }
            RestOrder::BottomRandom => {
                let mut rest = rest;
                self.rng.shuffle(&mut rest);
                for id in rest {
                    self.place(&mut t.ctx, id, Destination::BottomOfLibrary, who);
                }
            }
        }
    }

    // ---- costs paid during resolution ------------------------------------

    /// Whether `who` could take an "if they do" action now.
    fn can_do(&self, ctx: &EffectCtx, who: PlayerId, action: &Effect) -> bool {
        match action {
            Effect::Pay { cost, .. } => self.can_pay(ctx, who, cost),
            Effect::Discard { what, .. } => match what {
                DiscardSpec::Chosen { count, filter } => {
                    let n = self.amount(ctx, count) as usize;
                    self.player(who)
                        .hand
                        .iter()
                        .filter(|id| self.matches_card(ctx, **id, filter))
                        .count()
                        >= n
                }
                DiscardSpec::Hand | DiscardSpec::Random(_) => !self.player(who).hand.is_empty(),
            },
            Effect::Sacrifice { what, .. } => match what {
                ObjectSet::ChosenFrom { filter, count, .. } => {
                    self.own_permanents(ctx, who, filter).len() >= self.amount(ctx, count) as usize
                }
                other => !self.set_now(ctx, other).is_empty(),
            },
            _ => true,
        }
    }

    /// Whether `who` can pay every part of a cost (mana is checked against
    /// pool and untapped sources; mana is paid first, CR 601.2g–h).
    pub(crate) fn can_pay(&self, ctx: &EffectCtx, who: PlayerId, cost: &[CostPart]) -> bool {
        let except = taps_source(cost).then_some(ctx.source);
        cost.iter().all(|part| match part {
            CostPart::Mana(_) | CostPart::ManaRepeated { .. } => payable(
                &self.part_req(ctx, part),
                &self.usable_pool(who, None),
                &self.mana_sources(who, except),
            ),
            // CR 119.4: life can be paid only if the life total is at least the amount.
            // CR 119.4b: paying 0 life is always possible; otherwise not if
            // the life total can't change.
            CostPart::PayLife(q) => {
                let n = self.quantity(ctx, q);
                n <= 0
                    || (self.player(who).life >= n
                        && !self
                            .has_player_rule(who, mtg_cards::ir::PlayerRuleKind::LifeCantChange))
            }
            CostPart::Tap => self.obj(ctx.source).is_some_and(|o| !o.tapped),
            CostPart::Untap => self.obj(ctx.source).is_some_and(|o| o.tapped),
            CostPart::Loyalty(n) => {
                *n >= 0
                    || i64::from(self.counters_on(ctx.source, mtg_cards::ir::CounterKind::Loyalty))
                        >= i64::from(n.unsigned_abs())
            }
            CostPart::Sacrifice(set) => match set {
                ObjectSet::ChosenFrom { filter, count, .. } => {
                    self.own_permanents(ctx, who, filter).len() >= self.amount(ctx, count) as usize
                }
                other => self
                    .set_now(ctx, other)
                    .iter()
                    .any(|id| self.on_battlefield_now(*id)),
            },
            CostPart::Discard(spec) => match spec {
                DiscardSpec::Chosen { count, filter } => {
                    self.player(who)
                        .hand
                        .iter()
                        .filter(|id| self.matches_card(ctx, **id, filter))
                        .count()
                        >= self.amount(ctx, count) as usize
                }
                DiscardSpec::Hand => true,
                DiscardSpec::Random(q) => {
                    self.player(who).hand.len() >= self.amount(ctx, q) as usize
                }
            },
            // "another card" unless the filter is "this card" (Spirit Guides).
            CostPart::ExileFromHand(f) => self
                .player(who)
                .hand
                .iter()
                .any(|id| (f.this || *id != ctx.source) && self.matches_card(ctx, *id, f)),
            CostPart::ExileFromGraveyard { count, other } => {
                self.player(who)
                    .graveyard
                    .iter()
                    .filter(|id| !*other || **id != ctx.source)
                    .count()
                    >= *count as usize
            }
            CostPart::Exile { what, .. } => !self.own_permanents(ctx, who, what).is_empty(),
            CostPart::RemoveCounters { counter, amount } => {
                self.counters_on(ctx.source, *counter) >= *amount
            }
        })
    }

    fn pay_step(
        &mut self,
        t: &mut EffectTask,
        who: PlayerId,
        parts: Vec<CostPart>,
        next: u8,
    ) -> Flow {
        let Some(part) = parts.get(usize::from(next)).cloned() else {
            return Flow::Continue;
        };
        let except = taps_source(&parts).then_some(t.ctx.source);
        t.frames.push(Frame::Pay {
            who,
            parts,
            next: next + 1,
        });
        match part {
            CostPart::Mana(_) | CostPart::ManaRepeated { .. } => {
                let req = self.part_req(&t.ctx, &part);
                if req.is_paid() {
                    return Flow::Continue;
                }
                let plans = enumerate_plans(
                    &req,
                    &self.usable_pool(who, None),
                    &self.mana_sources(who, except),
                );
                let spell = t.ctx.source;
                Flow::Decide(who, DecisionKind::PayMana { spell, plans }, "601.2h")
            }
            CostPart::PayLife(q) => {
                let n = self.amount(&t.ctx, &q);
                self.lose_life(who, n);
                Flow::Continue
            }
            CostPart::Tap => {
                self.tap(t.ctx.source);
                Flow::Continue
            }
            CostPart::Untap => {
                self.untap(t.ctx.source);
                Flow::Continue
            }
            CostPart::Loyalty(n) => {
                let k = n.unsigned_abs();
                if n >= 0 {
                    self.add_counters(t.ctx.source, mtg_cards::ir::CounterKind::Loyalty, k);
                } else {
                    self.remove_counters(t.ctx.source, mtg_cards::ir::CounterKind::Loyalty, k);
                }
                Flow::Continue
            }
            CostPart::Sacrifice(set) => {
                if let ObjectSet::ChosenFrom { filter, count, .. } = &set {
                    let count = self.amount(&t.ctx, count);
                    let candidates = self.own_permanents(&t.ctx, who, filter);
                    t.frames.push(Frame::Pick(exact_pick(
                        who,
                        CardPurpose::Sacrifice,
                        candidates,
                        count,
                        AfterPick::Sacrifice,
                    )));
                } else {
                    let ids = self.set_now(&t.ctx, &set);
                    self.sacrifice_all(&mut t.ctx, &ids);
                }
                Flow::Continue
            }
            CostPart::Discard(spec) => {
                self.discard(t, who, &spec);
                Flow::Continue
            }
            CostPart::ExileFromHand(f) => {
                let candidates: Vec<ObjectId> = self
                    .player(who)
                    .hand
                    .iter()
                    .copied()
                    .filter(|id| {
                        (f.this || *id != t.ctx.source) && self.matches_card(&t.ctx, *id, &f)
                    })
                    .collect();
                t.frames.push(Frame::Pick(exact_pick(
                    who,
                    CardPurpose::Exile,
                    candidates,
                    1,
                    AfterPick::Exile {
                        remember: None,
                        face_down: false,
                    },
                )));
                Flow::Continue
            }
            CostPart::ExileFromGraveyard { count, other } => {
                let source = t.ctx.source;
                let candidates: Vec<ObjectId> = self
                    .player(who)
                    .graveyard
                    .iter()
                    .copied()
                    .filter(|id| !other || *id != source)
                    .collect();
                t.frames.push(Frame::Pick(exact_pick(
                    who,
                    CardPurpose::Exile,
                    candidates,
                    count,
                    AfterPick::Exile {
                        remember: None,
                        face_down: false,
                    },
                )));
                Flow::Continue
            }
            CostPart::RemoveCounters { counter, amount } => {
                self.remove_counters(t.ctx.source, counter, amount);
                Flow::Continue
            }
            CostPart::Exile { what, remember } => {
                let candidates = self.own_permanents(&t.ctx, who, &what);
                t.frames.push(Frame::Pick(exact_pick(
                    who,
                    CardPurpose::Exile,
                    candidates,
                    1,
                    AfterPick::Exile {
                        remember: Some(remember),
                        face_down: false,
                    },
                )));
                Flow::Continue
            }
        }
    }
}

fn node_or_unreachable<'p>(pool: &'p CardPool, a: AbilityRef, path: &[u8]) -> &'p Effect {
    node(pool, a, path).unwrap_or_else(|| unreachable!("frame path {path:?} is valid"))
}

fn set_var(ctx: &mut EffectCtx, store: String, v: i64) {
    ctx.vars.retain(|(k, _)| *k != store);
    ctx.vars.push((store, v));
}

/// A cost-payment frame, mana parts first (CR 601.2g: mana abilities are
/// activated before costs are paid; paying mana first keeps every other part payable).
/// Whether a cost includes {T} or {Q} of its source (CR 107.5–6), which keeps
/// the source out of the mana that pays the rest.
fn taps_source(cost: &[CostPart]) -> bool {
    cost.iter()
        .any(|c| matches!(c, CostPart::Tap | CostPart::Untap))
}

fn pay_frame(who: PlayerId, cost: &[CostPart]) -> Frame {
    let is_mana = |p: &&CostPart| matches!(p, CostPart::Mana(_) | CostPart::ManaRepeated { .. });
    let mut parts: Vec<CostPart> = cost.iter().filter(is_mana).cloned().collect();
    parts.extend(cost.iter().filter(|p| !is_mana(p)).cloned());
    Frame::Pay {
        who,
        parts,
        next: 0,
    }
}

/// A pick of exactly `count` cards (or all of them, if fewer).
fn exact_pick(
    who: PlayerId,
    purpose: CardPurpose,
    candidates: Vec<ObjectId>,
    count: u32,
    then: AfterPick,
) -> Pick {
    let n = count.min(u32::try_from(candidates.len()).unwrap_or(u32::MAX));
    Pick {
        who,
        purpose,
        candidates,
        picked: Vec::new(),
        max: n,
        min: n,
        then,
    }
}

fn purpose_for(to: Destination) -> CardPurpose {
    match to {
        Destination::Hand => CardPurpose::ReturnToHand,
        Destination::Battlefield | Destination::BattlefieldTapped => {
            CardPurpose::PutOntoBattlefield
        }
        Destination::TopOfLibrary => CardPurpose::PutOnTop,
        Destination::BottomOfLibrary => CardPurpose::PutOnBottom,
        Destination::Graveyard => CardPurpose::PutIntoGraveyard,
        Destination::Exile => CardPurpose::Exile,
    }
}

fn zone_matches(z: ZoneName, actual: ZoneId) -> bool {
    matches!(
        (z, actual),
        (ZoneName::Hand, ZoneId::Hand(_))
            | (ZoneName::Library, ZoneId::Library(_))
            | (ZoneName::Graveyard, ZoneId::Graveyard(_))
            | (ZoneName::Battlefield, ZoneId::Battlefield)
            | (ZoneName::Exile, ZoneId::Exile)
            | (ZoneName::Stack, ZoneId::Stack)
            | (ZoneName::Command, ZoneId::Command)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mana_bag_outcomes() {
        assert_eq!(
            mana_bags(&ManaSpec::Fixed("{B}{B}{B}".into())),
            vec![[0, 0, 3, 0, 0, 0]]
        );
        assert_eq!(mana_bags(&ManaSpec::AnyColor(Quantity::Fixed(1))).len(), 5);
        // Two mana in any combination of colors: 15 multisets of 5 colors.
        assert_eq!(mana_bags(&ManaSpec::AnyColor(Quantity::Fixed(2))).len(), 15);
        assert_eq!(
            mana_bags(&ManaSpec::AnyOneColor(Quantity::Fixed(3))).len(),
            5
        );
        assert_eq!(
            mana_bags(&ManaSpec::OneOf(vec!["{G}".into(), "{U}".into()])).len(),
            2
        );
    }
}
