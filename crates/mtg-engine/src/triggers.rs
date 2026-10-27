//! Triggered abilities (CR 603, `docs/03 Architecture/Subsystems/Triggered Abilities.md`,
//! M4 plan D1–D3).
//!
//! Every event the engine emits is matched against the trigger conditions of
//! permanents on the battlefield (and, for leaves-the-battlefield triggers,
//! of the object that just left: CR 603.10 look-back). Matches wait in a
//! pending queue; the next time a player would receive priority, after
//! state-based actions, they go on the stack in APNAP order, each player
//! ordering their own (CR 603.3b). The rest of putting a trigger on the stack
//! (targets, CR 603.3d) reuses the cast procedure.

use mtg_cards::ir::{Ability, ObjectFilter, StepName, Trigger, TriggeredAbility};
use mtg_core::{CardType, Color, ObjectId, PlayerId, Supertype};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::card::{DefId, ManaBag};
use crate::cast::{CastStage, CastTask};
use crate::decision::DecisionKind;
use crate::effects::{AbilityRef, EffectCtx, EffectTask, Lki, Path};
use crate::engine::{Game, Task};
use crate::events::Event;
use crate::state::{StackItem, Step, ZoneId};

/// What a trigger saw: the object and player from its trigger event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TriggerInfo {
    pub object: Option<ObjectId>,
    pub player: Option<PlayerId>,
    /// Last known information about the triggering object, if it left a zone.
    pub(crate) lki: Option<Lki>,
    /// The mana a permanent was tapped for (CR 605.1b triggers).
    pub(crate) mana: Option<ManaBag>,
    /// When the triggering object is an ability on the stack: its ability
    /// index on `object` (ward counters "that spell or ability").
    pub(crate) stack_ability: Option<u8>,
    /// "That much": the life gained or lost.
    pub(crate) amount: Option<u32>,
}

/// A delayed triggered ability (CR 603.7): created by an effect, it triggers
/// once on its event and then ceases to exist (CR 603.7c).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct DelayedTrigger {
    pub trigger: Trigger,
    /// Path of the effect to run, inside the creating ability.
    pub path: Path,
    /// The creating effect's context (its source, controller, targets and
    /// last known information; CR 603.7c, 603.7d).
    pub ctx: EffectCtx,
}

/// A triggered ability waiting to be put on the stack (CR 603.3).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct PendingTrigger {
    /// The source (possibly gone: look-back, CR 603.10).
    pub source: ObjectId,
    pub source_def: DefId,
    pub ability: u8,
    /// CR 603.3a: the source's controller when it triggered.
    pub controller: PlayerId,
    pub info: TriggerInfo,
    pub delayed: Option<Box<DelayedTrigger>>,
}

/// Putting pending triggers on the stack, player by player in APNAP order.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct StackTriggersTask {
    started: bool,
    order: SmallVec<[PlayerId; 8]>,
    next: u8,
}

impl StackTriggersTask {
    pub(crate) fn new() -> Self {
        Self {
            started: false,
            order: SmallVec::new(),
            next: 0,
        }
    }
}

/// A filter checked against a definition and controller rather than a live
/// object (for objects that have left, CR 603.10).
pub(crate) fn def_matches(
    g: &Game,
    def: DefId,
    controller: PlayerId,
    f: &ObjectFilter,
    you: PlayerId,
) -> bool {
    card_matches(g, g.def(def), controller, f, you)
}

/// [`def_matches`] for a definition in hand.
pub(crate) fn card_matches(
    g: &Game,
    d: &crate::card::CardDef,
    controller: PlayerId,
    f: &ObjectFilter,
    you: PlayerId,
) -> bool {
    let t = &d.type_line;
    let has_type = |w: &String| {
        CardType::ALL
            .iter()
            .any(|ct| ct.word().eq_ignore_ascii_case(w) && t.has_type(*ct))
    };
    let has_sub = |w: &String| t.subtypes.iter().any(|s| s.0.eq_ignore_ascii_case(w));
    let has_super = |w: &String| {
        Supertype::ALL
            .iter()
            .any(|st| st.word().eq_ignore_ascii_case(w) && t.has_supertype(*st))
    };
    (f.types.is_empty() || f.types.iter().any(has_type))
        && !f.not_types.iter().any(has_type)
        && (f.subtypes.is_empty() || f.subtypes.iter().any(has_sub))
        && !f.not_subtypes.iter().any(has_sub)
        && (f.supertypes.is_empty() || f.supertypes.iter().any(has_super))
        && !f.not_supertypes.iter().any(has_super)
        && f.color.as_ref().is_none_or(|letters| {
            letters
                .chars()
                .filter_map(Color::from_letter)
                .any(|c| d.colors.contains(c))
        })
        && g.player_rel(f.controller, you, controller)
        && f.token.is_none_or(|tok| tok == d.token)
}

pub(crate) fn step_is(s: StepName, step: Step) -> bool {
    matches!(
        (s, step),
        (StepName::Main, Step::PrecombatMain | Step::PostcombatMain)
            | (StepName::Upkeep, Step::Upkeep)
            | (StepName::Draw, Step::Draw)
            | (StepName::PrecombatMain, Step::PrecombatMain)
            | (StepName::PostcombatMain, Step::PostcombatMain)
            | (StepName::BeginningOfCombat, Step::BeginCombat)
            | (StepName::End, Step::End)
    )
}

impl Game {
    /// The triggered abilities on a definition, by ability index.
    fn triggered(&self, def: DefId) -> SmallVec<[(u8, &TriggeredAbility); 2]> {
        self.def(def)
            .printed_abilities()
            .iter()
            .enumerate()
            .filter_map(|(i, a)| match a {
                Ability::Triggered(t) => Some((u8::try_from(i).ok()?, t)),
                _ => None,
            })
            .collect()
    }

    /// The triggered abilities object `id` has now (CR 613, layer 6).
    fn triggered_of(&self, id: ObjectId) -> SmallVec<[(AbilityRef, &TriggeredAbility); 2]> {
        match self.chars(id) {
            Some(crate::layers::Chars::Printed { def_id, def, .. }) => def
                .printed_abilities()
                .iter()
                .enumerate()
                .filter_map(|(i, a)| match a {
                    Ability::Triggered(t) => Some((
                        AbilityRef {
                            def: def_id,
                            ability: u8::try_from(i).ok()?,
                        },
                        t,
                    )),
                    _ => None,
                })
                .collect(),
            Some(crate::layers::Chars::Computed(..)) => self
                .abilities_of(id)
                .into_iter()
                .filter_map(|(r, a)| match a {
                    Ability::Triggered(t) => Some((r, t)),
                    _ => None,
                })
                .collect(),
            None => SmallVec::new(),
        }
    }

    /// CR 603.2: queues every triggered ability that `e` triggers.
    pub(crate) fn match_triggers(&mut self, e: &Event) {
        let mut found: Vec<PendingTrigger> = Vec::new();
        // Permanents on the battlefield now.
        for &source in &self.battlefield {
            let Some(o) = self.obj(source) else { continue };
            // CR 613: the abilities it has now (layer 6).
            for (r, t) in self.triggered_of(source) {
                let (def, ability) = (r.def, r.ability);
                if let Some(info) = self.trigger_hit(&t.trigger, source, o.controller, e)
                    && self.intervening_if(t, source, def, ability, o.controller, info)
                {
                    found.push(PendingTrigger {
                        source,
                        source_def: def,
                        ability,
                        controller: o.controller,
                        info,
                        delayed: None,
                    });
                }
            }
        }
        // CR 603.10a: leaves-the-battlefield abilities "look back": the
        // permanent that just left, and every permanent leaving in the same
        // simultaneous batch, still see the event.
        if let Event::ZoneChange {
            from: ZoneId::Battlefield,
            def,
            controller,
            old,
            ..
        } = *e
        {
            let mut lookers: SmallVec<[(ObjectId, DefId, PlayerId); 4]> = SmallVec::new();
            lookers.push((old, def, controller));
            lookers.extend(
                self.leaving_batch
                    .iter()
                    .copied()
                    .filter(|(id, _, _)| *id != old && self.obj(*id).is_none()),
            );
            for (source, sdef, scontroller) in lookers {
                for (ability, t) in self.triggered(sdef) {
                    if matches!(
                        t.trigger,
                        Trigger::Dies(_) | Trigger::LeavesBattlefield(_) | Trigger::AnyOf(_)
                    ) && let Some(info) = self.trigger_hit(&t.trigger, source, scontroller, e)
                        && self.intervening_if(t, source, sdef, ability, scontroller, info)
                    {
                        found.push(PendingTrigger {
                            source,
                            source_def: sdef,
                            ability,
                            controller: scontroller,
                            info,
                            delayed: None,
                        });
                    }
                }
            }
        }
        // "When you cast this spell" works on the stack (CR 702.40a: storm).
        if let Event::SpellCast {
            player,
            object,
            def,
        } = *e
        {
            for (ability, t) in self.triggered(def) {
                if matches!(&t.trigger, Trigger::Cast { spell, .. } if spell.this)
                    && let Some(info) = self.trigger_hit(&t.trigger, object, player, e)
                {
                    found.push(PendingTrigger {
                        source: object,
                        source_def: def,
                        ability,
                        controller: player,
                        info,
                        delayed: None,
                    });
                }
            }
        }
        // CR 603.7c: a delayed trigger triggers only once.
        let mut i = 0;
        while i < self.delayed.len() {
            let d = &self.delayed[i];
            match self.trigger_hit(&d.trigger, d.ctx.source, d.ctx.controller, e) {
                Some(info) => {
                    let d = self.delayed.remove(i);
                    found.push(PendingTrigger {
                        source: d.ctx.source,
                        source_def: d.ctx.ability.def,
                        ability: d.ctx.ability.ability,
                        // CR 603.7d: controlled by the creating ability's controller.
                        controller: d.ctx.controller,
                        info,
                        delayed: Some(Box::new(d)),
                    });
                }
                None => i += 1,
            }
        }
        for p in found {
            self.log(Event::Triggered {
                source: p.source,
                def: p.source_def,
                ability: p.ability,
                controller: p.controller,
            });
            if p.delayed.is_none() && self.is_mana_trigger(p.source_def, p.ability) {
                // CR 605.4a: a triggered mana ability resolves immediately.
                let ctx = trigger_ctx(p.source_def, p.ability, p.source, p.controller, p.info);
                self.tasks
                    .push(Task::Effect(Box::new(EffectTask::new(ctx))));
            } else {
                self.pending_triggers.push(p);
            }
        }
    }

    /// CR 605.1b: a triggered ability that triggers from a mana ability and
    /// could add mana is a mana ability.
    fn is_mana_trigger(&self, def: DefId, ability: u8) -> bool {
        matches!(
            self.def(def).abilities.get(usize::from(ability)),
            Some(Ability::Triggered(TriggeredAbility {
                trigger: Trigger::TapsForMana { .. },
                ..
            }))
        )
    }

    /// CR 603.4: an intervening "if" must hold when the ability triggers.
    fn intervening_if(
        &self,
        t: &TriggeredAbility,
        source: ObjectId,
        def: DefId,
        ability: u8,
        controller: PlayerId,
        info: TriggerInfo,
    ) -> bool {
        t.condition.as_ref().is_none_or(|c| {
            let ctx = trigger_ctx(def, ability, source, controller, info);
            self.condition(&ctx, c)
        })
    }

    /// Whether `trigger`, on a source controlled by `you`, matches event `e`;
    /// returns what it saw.
    fn trigger_hit(
        &self,
        trigger: &Trigger,
        source: ObjectId,
        you: PlayerId,
        e: &Event,
    ) -> Option<TriggerInfo> {
        let seen = |object: Option<ObjectId>, player: Option<PlayerId>| TriggerInfo {
            object,
            player,
            lki: None,
            mana: None,
            stack_ability: None,
            amount: None,
        };
        match (trigger, e) {
            (Trigger::AnyOf(v), _) => v.iter().find_map(|t| self.trigger_hit(t, source, you, e)),
            // CR 603.6a
            (
                Trigger::Enters(f),
                Event::ZoneChange {
                    to: ZoneId::Battlefield,
                    new: id,
                    ..
                }
                | Event::TokenCreated { object: id, .. },
            ) => {
                let o = self.obj(*id)?;
                let ok = if f.this {
                    *id == source
                } else {
                    self.matches_filter(*id, f, you, Some(source))
                };
                // Torpor Orb (CR 613.11): creatures entering don't trigger.
                (ok && self.entering_triggers(*id)).then(|| seen(Some(*id), Some(o.controller)))
            }
            // CR 700.4: "dies" means put into a graveyard from the battlefield.
            (
                Trigger::Dies(f) | Trigger::LeavesBattlefield(f),
                Event::ZoneChange {
                    from: ZoneId::Battlefield,
                    to,
                    def,
                    owner,
                    controller,
                    power,
                    toughness,
                    old,
                    ..
                },
            ) => {
                if matches!(trigger, Trigger::Dies(_)) && !matches!(to, ZoneId::Graveyard(_)) {
                    return None;
                }
                let ok = if f.this {
                    *old == source
                } else {
                    (!f.other || *old != source) && def_matches(self, *def, *controller, f, you)
                };
                ok.then_some(TriggerInfo {
                    mana: None,
                    stack_ability: None,
                    amount: None,
                    object: Some(*old),
                    player: Some(*controller),
                    lki: Some(Lki {
                        id: *old,
                        def: *def,
                        owner: *owner,
                        controller: *controller,
                        zone: ZoneId::Battlefield,
                        power: *power,
                        toughness: *toughness,
                    }),
                })
            }
            (
                Trigger::Cast {
                    by,
                    spell,
                    first_each_turn,
                    second_each_turn,
                },
                Event::SpellCast { player, object, .. },
            ) => {
                // "their first/second [spell] each turn": the count so far.
                let nth = || {
                    self.cast_this_turn
                        .iter()
                        .filter(|r| {
                            r.player == *player && def_matches(self, r.def, *player, spell, you)
                        })
                        .count()
                };
                let ok = self.player_rel(*by, you, *player)
                    && self.matches_filter(*object, spell, you, Some(source))
                    && (!*first_each_turn || nth() == 1)
                    && (!*second_each_turn || nth() == 2);
                ok.then(|| TriggerInfo {
                    // CR 702.40a: the spells cast before it this turn (storm).
                    amount: self
                        .cast_this_turn
                        .iter()
                        .position(|r| r.object == *object)
                        .and_then(|i| u32::try_from(i).ok()),
                    ..seen(Some(*object), Some(*player))
                })
            }
            (
                Trigger::Draws {
                    by,
                    except_first_in_draw_step,
                },
                Event::Drew {
                    player,
                    object,
                    first_in_draw_step,
                },
            ) => (self.player_rel(*by, you, *player)
                && !(*except_first_in_draw_step && *first_in_draw_step))
                .then(|| seen(Some(*object), Some(*player))),
            (Trigger::Discards { by }, Event::Discarded { player, object }) => self
                .player_rel(*by, you, *player)
                .then(|| seen(Some(*object), Some(*player))),
            // CR 603.2b
            (Trigger::BeginningOf { step, whose }, Event::StepBegan { step: now }) => {
                let active = self.turn.active;
                (step_is(*step, *now) && self.player_rel(*whose, you, active))
                    .then(|| seen(None, Some(active)))
            }
            (Trigger::Searches { by }, Event::Searched { player, .. }) => self
                .player_rel(*by, you, *player)
                .then(|| seen(None, Some(*player))),
            (Trigger::BecomesTapped(f), Event::Tapped { object }) => {
                let ok = if f.this {
                    *object == source
                } else {
                    self.matches_filter(*object, f, you, Some(source))
                };
                ok.then(|| seen(Some(*object), None))
            }
            (Trigger::Attacks(f), Event::AttackerDeclared { object, defending }) => {
                let ok = if f.this {
                    *object == source
                } else {
                    self.matches_filter(*object, f, you, Some(source))
                };
                ok.then(|| seen(Some(*object), Some(*defending)))
            }
            (
                Trigger::DealsCombatDamageToPlayer(f),
                Event::Damage {
                    source: by,
                    target: crate::state::Target::Player(p),
                    combat: true,
                    ..
                },
            ) => {
                let ok = if f.this {
                    *by == source
                } else {
                    self.matches_filter(*by, f, you, Some(source))
                };
                ok.then(|| seen(Some(*by), Some(*p)))
            }
            // CR 605.1b
            (
                Trigger::TapsForMana { by, what },
                Event::TappedForMana {
                    player,
                    source: tapped,
                    produced,
                },
            ) => (self.player_rel(*by, you, *player)
                && self.matches_filter(*tapped, what, you, Some(source)))
            .then_some(TriggerInfo {
                object: Some(*tapped),
                player: Some(*player),
                lki: None,
                mana: Some(*produced),
                stack_ability: None,
                amount: None,
            }),
            // CR 119.9, 119.3
            (Trigger::GainsLife { by }, Event::LifeGained { player, amount })
            | (Trigger::LosesLife { by }, Event::LifeLost { player, amount }) => {
                let kind_ok = matches!(
                    (trigger, e),
                    (Trigger::GainsLife { .. }, Event::LifeGained { .. })
                        | (Trigger::LosesLife { .. }, Event::LifeLost { .. })
                );
                (kind_ok && self.player_rel(*by, you, *player)).then_some(TriggerInfo {
                    object: None,
                    player: Some(*player),
                    lki: None,
                    mana: None,
                    stack_ability: None,
                    amount: Some(*amount),
                })
            }
            // CR 702.21a
            (
                Trigger::BecomesTarget { by, what },
                Event::BecameTarget {
                    target,
                    source: by_source,
                    ability,
                    controller,
                },
            ) => {
                let ok = self.player_rel(*by, you, *controller)
                    && if what.this {
                        *target == source
                    } else {
                        self.matches_filter(*target, what, you, Some(source))
                    };
                ok.then_some(TriggerInfo {
                    object: Some(*by_source),
                    player: Some(*controller),
                    lki: None,
                    mana: None,
                    stack_ability: *ability,
                    amount: None,
                })
            }
            _ => None,
        }
    }

    /// Starts a batch of permanents leaving the battlefield simultaneously
    /// (CR 603.10a). Only matters when something can trigger.
    pub(crate) fn begin_leaving(&mut self, ids: &[ObjectId]) {
        if !self.pool.has_triggers() {
            return;
        }
        self.leaving_batch = ids
            .iter()
            .filter_map(|id| {
                let o = self.obj(*id)?;
                (o.zone == ZoneId::Battlefield).then_some((*id, o.def, o.controller))
            })
            .collect();
    }

    pub(crate) fn end_leaving(&mut self) {
        self.leaving_batch.clear();
    }

    // ---- putting triggers on the stack (CR 603.3) --------------------------

    pub(crate) fn stack_triggers_step(&mut self, mut t: StackTriggersTask) {
        // CR 800.4d: abilities controlled by players who left don't go on the stack.
        self.pending_triggers
            .retain(|p| self.players[p.controller.0 as usize].in_game());
        if self.pending_triggers.is_empty() {
            return;
        }
        if !t.started {
            t.started = true;
            t.order = self.turn_order_from(self.turn.active);
            t.next = 0;
        }
        let Some(&p) = t.order.get(usize::from(t.next)) else {
            // Abilities that triggered while these went on the stack go next (CR 603.3b).
            self.tasks
                .push(Task::StackTriggers(StackTriggersTask::new()));
            return;
        };
        let mine: Vec<usize> = self
            .pending_triggers
            .iter()
            .enumerate()
            .filter(|(_, x)| x.controller == p)
            .map(|(i, _)| i)
            .collect();
        match mine.as_slice() {
            [] => {
                t.next += 1;
                self.tasks.push(Task::StackTriggers(t));
            }
            [only] => {
                let only = *only;
                self.tasks.push(Task::StackTriggers(t));
                self.put_trigger(only);
            }
            _ => {
                let triggers = mine
                    .iter()
                    .map(|i| {
                        let x = &self.pending_triggers[*i];
                        (x.source, x.source_def, x.ability)
                    })
                    .collect();
                self.tasks.push(Task::StackTriggers(t));
                self.decide(p, DecisionKind::OrderTriggers { triggers }, "603.3b");
            }
        }
    }

    /// The player picked which of their triggers goes on the stack next.
    pub(crate) fn stack_triggers_answer(&mut self, t: StackTriggersTask, option: usize) {
        let p = t.order[usize::from(t.next)];
        let index = self
            .pending_triggers
            .iter()
            .enumerate()
            .filter(|(_, x)| x.controller == p)
            .nth(option)
            .map(|(i, _)| i);
        self.tasks.push(Task::StackTriggers(t));
        if let Some(i) = index {
            self.put_trigger(i);
        }
    }

    /// Puts pending trigger `i` on the stack; targets are chosen through the
    /// cast procedure (CR 603.3d). One with an unsatisfiable target is removed.
    fn put_trigger(&mut self, i: usize) {
        let pt = self.pending_triggers.remove(i);
        if let Some(d) = pt.delayed {
            // Delayed triggers have no targets of their own (M4 limitation).
            let mut item =
                StackItem::new(pt.source, pt.source_def, Some(pt.ability), pt.controller);
            item.trigger = Some(pt.info);
            item.delayed = Some(d);
            self.stack.push(item);
            return;
        }
        let Some(Ability::Triggered(ta)) = self
            .def(pt.source_def)
            .abilities
            .get(usize::from(pt.ability))
        else {
            return;
        };
        let src = self.def(pt.source_def);
        let satisfiable = ta.targets.iter().all(|s| {
            s.up_to.is_some()
                || !self
                    .legal_targets(&s.what, pt.controller, None, Some(src))
                    .is_empty()
        });
        if !satisfiable {
            self.log(Event::TriggerRemoved {
                source: pt.source,
                def: pt.source_def,
                ability: pt.ability,
            });
            return;
        }
        let mut item = StackItem::new(pt.source, pt.source_def, Some(pt.ability), pt.controller);
        item.trigger = Some(pt.info);
        self.stack.push(item);
        self.tasks.push(Task::Cast(CastTask {
            spell: pt.source,
            caster: pt.controller,
            stage: CastStage::Targets,
            index: 0,
        }));
    }
}

/// The resolution context of a triggered ability.
pub(crate) fn trigger_ctx(
    def: DefId,
    ability: u8,
    source: ObjectId,
    controller: PlayerId,
    info: TriggerInfo,
) -> EffectCtx {
    let mut ctx = EffectCtx::new(AbilityRef { def, ability }, source, controller);
    ctx.triggering_object = info.object;
    ctx.triggering_player = info.player;
    ctx.triggering_mana = info.mana;
    ctx.triggering_ability = info.stack_ability;
    ctx.triggering_amount = info.amount;
    if let Some(l) = info.lki {
        ctx.lki.push(l);
    }
    ctx
}
