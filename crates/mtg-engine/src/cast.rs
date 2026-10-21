//! Casting spells (CR 601.2a–i) as a resumable procedure
//! (`docs/03 Architecture/Subsystems/Casting & Activating.md`).
//!
//! Castability is checked exactly before a spell is offered (M2 plan, D7):
//! every target slot has a legal target and some payment exists. So the
//! CR 601.2e legality check can't fail, and no rewind is needed in M2.

use mtg_core::{CardType, ObjectId, PlayerId};
use serde::{Deserialize, Serialize};

use crate::card::ManaType;
use crate::decision::{DecisionKind, ManaStep};
use crate::engine::{Game, Task};
use crate::events::Event;
use crate::mana::{CostReq, enumerate_plans, payable};
use crate::state::{StackItem, ZoneId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum CastStage {
    /// CR 601.2c
    Targets,
    /// CR 601.2f–h
    Pay,
    /// CR 601.2i
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct CastTask {
    pub spell: ObjectId,
    pub caster: PlayerId,
    pub stage: CastStage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct ManualPayTask {
    pub spell: ObjectId,
    pub payer: PlayerId,
    pub remaining: CostReq,
}

impl Game {
    /// Whether `p` may begin casting card `id` from hand right now, with a
    /// complete legal cast guaranteed to exist.
    pub(crate) fn can_cast(&self, p: PlayerId, id: ObjectId, sorcery_timing: bool) -> bool {
        let Some(def) = self.obj_def(id) else {
            return false;
        };
        // CR 202.1b: no mana cost means it can't be cast this way.
        let Some(cost) = &def.mana_cost else {
            return false;
        };
        // CR 117.1a: instants any time with priority; everything else at sorcery timing.
        if !def.is(CardType::Instant) && !sorcery_timing {
            return false;
        }
        // CR 601.2c: every required target slot needs a legal target.
        if let Some(spell) = def.spell()
            && spell
                .targets
                .iter()
                .any(|t| t.up_to.is_none() && self.legal_targets(&t.what, p).is_empty())
        {
            return false;
        }
        payable(
            &CostReq::from_cost(cost),
            &self.player(p).pool,
            &self.mana_sources(p, None),
        )
    }

    /// CR 601.2a: move the card to the stack and start the procedure.
    pub(crate) fn begin_cast(&mut self, p: PlayerId, card: ObjectId) {
        self.turn.priority = Some(p);
        let Some(spell) = self.move_object(card, ZoneId::Stack, true) else {
            return;
        };
        self.stack.push(StackItem {
            object: spell,
            controller: p,
            targets: smallvec::SmallVec::new(),
        });
        self.tasks.push(Task::Cast(CastTask {
            spell,
            caster: p,
            stage: CastStage::Targets,
        }));
    }

    pub(crate) fn cast_step(&mut self, t: CastTask) {
        let Some(def) = self.obj_def(t.spell).cloned() else {
            return;
        };
        match t.stage {
            CastStage::Targets => {
                // CR 601.2c
                if let Some(slot) = def.spell().and_then(|s| s.targets.first()) {
                    let targets = self.legal_targets(&slot.what, t.caster);
                    debug_assert!(
                        !targets.is_empty(),
                        "castability pre-check guarantees a target"
                    );
                    self.tasks.push(Task::Cast(t));
                    self.decide(
                        t.caster,
                        DecisionKind::ChooseTarget {
                            spell: t.spell,
                            targets,
                        },
                        "601.2c",
                    );
                } else {
                    self.tasks.push(Task::Cast(CastTask {
                        stage: CastStage::Pay,
                        ..t
                    }));
                }
            }
            CastStage::Pay => {
                // CR 601.2f: total cost (no modifiers in M2); CR 601.2g–h: activate mana abilities and pay.
                let req = def
                    .mana_cost
                    .as_ref()
                    .map(CostReq::from_cost)
                    .unwrap_or_default();
                if req.is_paid() {
                    self.tasks.push(Task::Cast(CastTask {
                        stage: CastStage::Done,
                        ..t
                    }));
                    return;
                }
                let plans = enumerate_plans(
                    &req,
                    &self.player(t.caster).pool,
                    &self.mana_sources(t.caster, None),
                );
                self.tasks.push(Task::Cast(t));
                self.decide(
                    t.caster,
                    DecisionKind::PayMana {
                        spell: t.spell,
                        plans,
                    },
                    "601.2h",
                );
            }
            CastStage::Done => {
                // CR 601.2i: the spell becomes cast.
                if let Some(o) = self.obj(t.spell) {
                    let def = o.def;
                    self.log(Event::SpellCast {
                        player: t.caster,
                        object: t.spell,
                        def,
                    });
                }
            }
        }
    }

    pub(crate) fn cast_answer(&mut self, t: CastTask, kind: DecisionKind, option: usize) {
        match kind {
            DecisionKind::ChooseTarget { targets, .. } => {
                if let Some(item) = self.stack.iter_mut().find(|s| s.object == t.spell) {
                    item.targets.push(targets[option]);
                }
                self.tasks.push(Task::Cast(CastTask {
                    stage: CastStage::Pay,
                    ..t
                }));
            }
            DecisionKind::PayMana { plans, .. } => {
                self.tasks.push(Task::Cast(CastTask {
                    stage: CastStage::Done,
                    ..t
                }));
                if let Some(plan) = plans.get(option) {
                    self.execute_plan(t.caster, plan);
                } else {
                    let remaining = self
                        .obj_def(t.spell)
                        .and_then(|d| d.mana_cost.as_ref())
                        .map(CostReq::from_cost)
                        .unwrap_or_default();
                    self.tasks.push(Task::ManualPay(ManualPayTask {
                        spell: t.spell,
                        payer: t.caster,
                        remaining,
                    }));
                }
            }
            other => unreachable!("cast procedure got {other:?}"),
        }
    }

    /// Manual payment steps that keep the payment completable.
    pub(crate) fn manual_steps(&self, t: &ManualPayTask) -> Vec<ManaStep> {
        let pool = self.player(t.payer).pool;
        let sources = self.mana_sources(t.payer, None);
        let mut steps = Vec::new();
        for m in ManaType::ALL {
            if pool.get(m) == 0 {
                continue;
            }
            let mut rem = t.remaining;
            if !rem.pay_one(m) {
                continue;
            }
            let mut p2 = pool;
            p2.0[m.index()] -= 1;
            if payable(&rem, &p2, &sources) {
                steps.push(ManaStep::Spend(m));
            }
        }
        for (i, s) in sources.iter().enumerate() {
            for (ability, m) in &s.abilities {
                let mut p2 = pool;
                p2.add(*m, 1);
                let rest: Vec<_> = sources
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, x)| x.clone())
                    .collect();
                if payable(&t.remaining, &p2, &rest) {
                    steps.push(ManaStep::Activate {
                        source: s.object,
                        ability: *ability,
                    });
                }
            }
        }
        steps
    }

    pub(crate) fn manual_pay_step(&mut self, t: ManualPayTask) {
        if t.remaining.is_paid() {
            return;
        }
        let steps = self.manual_steps(&t);
        debug_assert!(
            !steps.is_empty(),
            "a feasible payment always has a next step"
        );
        self.tasks.push(Task::ManualPay(t));
        self.decide(
            t.payer,
            DecisionKind::ManaStep {
                spell: t.spell,
                steps,
            },
            "601.2g",
        );
    }

    pub(crate) fn manual_pay_answer(&mut self, mut t: ManualPayTask, step: ManaStep) {
        match step {
            ManaStep::Activate { source, ability } => self.activate_mana(source, ability),
            ManaStep::Spend(m) => {
                if t.remaining.pay_one(m) {
                    self.player_mut(t.payer).pool.0[m.index()] -= 1;
                }
            }
        }
        self.tasks.push(Task::ManualPay(t));
    }
}
