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
    /// This effect is a mana ability whose cost taps its source, so the
    /// mana it adds is "tapped for mana" (CR 106.12, 605.1b triggers).
    pub tapped_for_mana: bool,
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
            tapped_for_mana: false,
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
    Exile { remember: Option<String> },
    /// Search results (CR 701.23): reveal if asked, then put them `to`.
    Search { to: Destination, reveal: bool },
    /// Put this many +1/+1 counters on the pick (amass, CR 701.47a).
    PutCounters(u32),
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
}

/// The root effect of an ability.
pub(crate) fn root(pool: &CardPool, a: AbilityRef) -> Option<&Effect> {
    match pool.get(a.def).abilities.get(a.ability as usize)? {
        Ability::Spell(s) => Some(&s.effect),
        Ability::Activated(x) => Some(&x.effect),
        Ability::Triggered(t) => Some(&t.effect),
        _ => None,
    }
}

/// Child `i` of an effect node; the numbering matches [`Frame`] paths.
fn child(e: &Effect, i: u8) -> Option<&Effect> {
    let i = usize::from(i);
    match e {
        Effect::Seq(v) => v.get(i),
        Effect::Modal { modes, .. } => modes.get(i),
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
                    self.execute_plan(who, plan);
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
                            self.place(&mut t.ctx, id, to, p.who);
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
                    other => unreachable!("choice frame answered with {other:?}"),
                };
                self.log(Event::Chose {
                    player: who,
                    choice: text,
                });
            }
            (frame, kind) => unreachable!("effect frame {frame:?} answered with {kind:?}"),
        }
        self.tasks.push(Task::Effect(t));
    }

    fn exec_frame(&mut self, t: &mut EffectTask, frame: Frame) -> Flow {
        match frame {
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
            Frame::Place { ids, to, who } => {
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
            Effect::Exile { what, remember, .. } => {
                let remember = remember.clone();
                if let Some(p) = self.pick_for(t, what, CardPurpose::Exile) {
                    t.frames.push(Frame::Pick(Pick {
                        then: AfterPick::Exile { remember },
                        ..p
                    }));
                    return Flow::Continue;
                }
                let ids = self.set_now(&t.ctx, what);
                self.exile_all(&mut t.ctx, &ids, remember);
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
                for id in self.set_now(&t.ctx, what) {
                    self.place(&mut t.ctx, id, to, who);
                }
            }
            Effect::Pump {
                what,
                power,
                toughness,
            } => {
                for id in self.set_now(&t.ctx, what) {
                    if self.on_battlefield_now(id) {
                        // CR 611.2a: until-end-of-turn effects end in cleanup (CR 514.2).
                        self.pt_effects.push((id, *power, *toughness));
                    }
                }
            }
            Effect::RevealUntil {
                who,
                filter,
                found_to,
                rest_to,
            } => {
                if let Some(p) = self.player_ref(&t.ctx, who) {
                    self.reveal_until(&mut t.ctx, p, filter, *found_to, *rest_to);
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
                for id in self.set_now(&t.ctx, set) {
                    self.counter(&mut t.ctx, id);
                }
            }
            Effect::Draw { who, amount } => {
                let n = self.amount(&t.ctx, amount);
                if let Some(p) = self.player_ref(&t.ctx, who) {
                    for _ in 0..n {
                        self.draw(p);
                    }
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
            Effect::AddMana { mana, .. } => {
                let who = t.ctx.each.unwrap_or(t.ctx.controller);
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
                let candidates: Vec<ObjectId> = library
                    .iter()
                    .rev()
                    .copied()
                    .filter(|id| self.matches_card(&t.ctx, *id, filter))
                    .collect();
                // CR 701.23b/d: a search for a stated quality may fail to find;
                // a search for a quantity of cards must find as many as possible.
                let quality = *filter != ObjectFilter::default();
                let n = count.min(u32::try_from(candidates.len()).unwrap_or(u32::MAX));
                t.frames.push(Frame::Pick(Pick {
                    who: p,
                    purpose: CardPurpose::Search,
                    candidates,
                    picked: Vec::new(),
                    max: n,
                    min: if quality { 0 } else { n },
                    then: AfterPick::Search {
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
            } => {
                let n = self.amount(&t.ctx, count);
                if let (Some(def), Some(p)) =
                    (self.pool.token(token), self.player_ref(&t.ctx, controller))
                {
                    self.create_tokens(def, p, n);
                }
            }
            Effect::Escape(x) => {
                // `support::check` admits only registered escapes.
                if let Some(f) = crate::escapes::lookup(&x.function) {
                    f(self, t);
                }
            }
            // `support::check` rejects these until their phase lands.
            Effect::EntersTapped => {
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
            ObjectRef::Target(n) => ctx
                .targets
                .get(usize::from(*n))
                .map(|slot| {
                    slot.iter()
                        .filter_map(|t| match t {
                            Target::Object(id) => Some(*id),
                            Target::Player(_) => None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            ObjectRef::Remembered(name) => ctx
                .remembered
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
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
                    Target::Object(_) => None,
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
    fn amount(&self, ctx: &EffectCtx, q: &Quantity) -> u32 {
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
                QuantityExpr::Var(name) => ctx
                    .vars
                    .iter()
                    .find(|(k, _)| k == name)
                    .map_or(0, |(_, v)| *v),
                QuantityExpr::Plus(q, n) => self.quantity(ctx, q) + n,
                QuantityExpr::CountersOn { what, counter } => self
                    .object_ref(ctx, what)
                    .map_or(0, |id| i64::from(self.counters_on(id, *counter))),
                // Rejected by `support::check` until combat history is tracked (M4).
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
            Condition::AtLeast(a, b) => self.quantity(ctx, a) >= self.quantity(ctx, b),
            Condition::Not(c) => !self.condition(ctx, c),
            // Rejected by `support::check` until the command zone exists (M6).
            Condition::ControlCommander => false,
        }
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
        self.obj(id).is_some_and(|o| o.zone == ZoneId::Battlefield)
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
        self.move_object(id, to, top)
    }

    /// Puts an object into a destination. Cards already in their owner's
    /// library are reordered in place (moving within a zone isn't a zone change).
    fn place(&mut self, ctx: &mut EffectCtx, id: ObjectId, to: Destination, who: PlayerId) {
        let Some(o) = self.obj(id).copied() else {
            return;
        };
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
            Destination::Battlefield => {
                // CR 110.2: it enters under the control of the player putting it there.
                if let Some(new) = self.fx_move(ctx, id, ZoneId::Battlefield, true)
                    && let Some(n) = self.objects.get_mut(new)
                {
                    n.controller = who;
                }
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

    fn exile_all(&mut self, ctx: &mut EffectCtx, ids: &[ObjectId], remember: Option<String>) {
        let mut new_ids = SmallVec::new();
        for &id in ids {
            if self.obj(id).is_some_and(|o| o.zone != ZoneId::Exile)
                && let Some(new) = self.fx_move(ctx, id, ZoneId::Exile, true)
            {
                new_ids.push(new);
            }
        }
        if let Some(name) = remember {
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
        let Some(o) = self.obj(id).copied() else {
            return;
        };
        // CR 101.2: "can't be countered" takes precedence.
        if o.zone != ZoneId::Stack || self.def(o.def).cant_be_countered() {
            return;
        }
        self.log(Event::Countered { object: id });
        self.fx_move(ctx, id, ZoneId::Graveyard(o.owner), true);
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
        for &id in ids {
            if let Some(new) = self.fx_move(ctx, id, ZoneId::Graveyard(p), true) {
                n += 1;
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
        if ctx.tapped_for_mana && self.pool.has_triggers() {
            self.log(Event::TappedForMana {
                player: who,
                source: ctx.source,
                produced: *bag,
            });
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
            [one] => self.add_counters(*one, mtg_cards::ir::CounterKind::PlusOne, n),
            _ => t.frames.push(Frame::Pick(exact_pick(
                who,
                CardPurpose::PutCounters,
                armies,
                1,
                AfterPick::PutCounters(n),
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
        ctx: &mut EffectCtx,
        p: PlayerId,
        filter: &ObjectFilter,
        found_to: Destination,
        rest_to: Destination,
    ) {
        let mut revealed = Vec::new();
        let mut found = None;
        for id in self.player(p).library.iter().rev().copied() {
            revealed.push(id);
            if self.matches_card(ctx, id, filter) {
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
            self.place(ctx, f, found_to, p);
        }
        if rest_to == Destination::BottomOfLibrary {
            // The rest go to the bottom in a random order.
            self.rng.shuffle(&mut rest);
        }
        for id in rest {
            self.place(ctx, id, rest_to, p);
        }
    }

    fn win_by_effect(&mut self, p: PlayerId) {
        // CR 104.2b: an effect states that a player wins; the game ends.
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
                        self.place(&mut t.ctx, id, to, p.who);
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
            AfterPick::Sacrifice => self.sacrifice_all(&mut t.ctx, &picked),
            AfterPick::PutCounters(n) => {
                for id in picked {
                    self.add_counters(id, mtg_cards::ir::CounterKind::PlusOne, n);
                }
            }
            AfterPick::Exile { remember } => self.exile_all(&mut t.ctx, &picked, remember),
            AfterPick::Search { to, reveal } => {
                self.log(Event::Searched {
                    player: who,
                    found: u32::try_from(picked.len()).unwrap_or(0),
                });
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
                        self.place(&mut t.ctx, id, to, who);
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
                        self.place(&mut t.ctx, id, to, who);
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
                &self.player(who).pool,
                &self.mana_sources(who, except),
            ),
            // CR 119.4: life can be paid only if the life total is at least the amount.
            CostPart::PayLife(q) => self.player(who).life >= self.quantity(ctx, q),
            CostPart::Tap => self.obj(ctx.source).is_some_and(|o| !o.tapped),
            CostPart::Untap => self.obj(ctx.source).is_some_and(|o| o.tapped),
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
            CostPart::ExileFromHand(f) => self
                .player(who)
                .hand
                .iter()
                .any(|id| *id != ctx.source && self.matches_card(ctx, *id, f)),
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
                    &self.player(who).pool,
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
                    .filter(|id| *id != t.ctx.source && self.matches_card(&t.ctx, *id, &f))
                    .collect();
                t.frames.push(Frame::Pick(exact_pick(
                    who,
                    CardPurpose::Exile,
                    candidates,
                    1,
                    AfterPick::Exile { remember: None },
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
                    AfterPick::Exile { remember: None },
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
        Destination::Battlefield => CardPurpose::PutOntoBattlefield,
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
