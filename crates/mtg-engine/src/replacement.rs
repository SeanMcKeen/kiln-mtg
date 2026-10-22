//! Replacement effects (CR 614, 616) and the rule statics M4 needs
//! (`docs/03 Architecture/Subsystems/Replacement & Prevention.md`, M4 plan R1–R6).
//!
//! - Draws a replacement could change are [`DrawTask`]s: one applicable
//!   replacement applies; several are a `ChooseReplacement` decision for the
//!   affected player (CR 616.1). The replacements already applied travel with
//!   the modified event in the effect context (CR 614.5, 616.1f).
//! - "Enters tapped" modifies the zone move itself (CR 614.1c–d, 614.12).
//! - "Instead" replacements of entering (Mox Diamond) run as an effect in place
//!   of the move.
//! - "Skip your [step]" (CR 614.10) and "doesn't untap" (CR 502.3) are checks.

use mtg_cards::ir::{self, Ability, Effect, ReplaceEvent, StaticEffect};
use mtg_core::{ObjectId, PlayerId};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::card::DefId;
use crate::decision::DecisionKind;
use crate::effects::{AbilityRef, EffectCtx, EffectTask};
use crate::engine::{Game, Task};
use crate::events::Event;
use crate::state::Step;
use crate::triggers::{def_matches, step_is};

/// One replacement ability of one object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReplacementRef {
    pub source: ObjectId,
    pub def: DefId,
    pub ability: u8,
}

/// Replacements already applied to an event and the events that replaced it (CR 614.5).
pub(crate) type Applied = SmallVec<[ReplacementRef; 1]>;

/// Drawing one card, which a replacement effect may change (CR 614.11).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct DrawTask {
    pub player: PlayerId,
    pub applied: Applied,
}

impl Game {
    /// The replacement abilities of a definition, by ability index.
    fn replacements(&self, def: DefId) -> impl Iterator<Item = (u8, &ir::ReplacementAbility)> {
        self.def(def)
            .abilities
            .iter()
            .enumerate()
            .filter_map(|(i, a)| match a {
                Ability::Replacement(r) => Some((u8::try_from(i).ok()?, r)),
                _ => None,
            })
    }

    /// The static abilities of permanents on the battlefield, with their
    /// source and controller. Conditional statics aren't supported yet.
    fn statics(&self) -> impl Iterator<Item = (ObjectId, PlayerId, &StaticEffect)> {
        self.battlefield
            .iter()
            .filter_map(|&id| self.obj(id).map(|o| (id, o)))
            .flat_map(move |(id, o)| {
                self.def(o.def)
                    .abilities
                    .iter()
                    .filter_map(move |a| match a {
                        Ability::Static(s) if s.condition.is_none() => {
                            Some((id, o.controller, &s.effect))
                        }
                        _ => None,
                    })
            })
    }

    /// The resolution context of a replacement's "instead" effect; the
    /// affected player is its "that player".
    fn replacement_ctx(
        &self,
        r: ReplacementRef,
        controller: PlayerId,
        affected: PlayerId,
        applied: &[ReplacementRef],
    ) -> EffectCtx {
        let mut ctx = EffectCtx::new(
            AbilityRef {
                def: r.def,
                ability: r.ability,
            },
            r.source,
            controller,
        );
        ctx.triggering_player = Some(affected);
        ctx.applied = applied.iter().copied().chain([r]).collect();
        ctx
    }

    // ---- draws (CR 614.11) ---------------------------------------------------

    /// The replacement effects that would apply to `p` drawing a card now.
    pub(crate) fn draw_replacements(
        &self,
        p: PlayerId,
        applied: &[ReplacementRef],
    ) -> SmallVec<[ReplacementRef; 2]> {
        let mut out = SmallVec::new();
        if !self.pool.has_replacements() {
            return out;
        }
        let first_in_draw_step =
            p == self.turn.active && self.turn.step == Step::Draw && !self.drew_in_draw_step;
        for &source in &self.battlefield {
            let Some(o) = self.obj(source) else { continue };
            for (ability, r) in self.replacements(o.def) {
                let ReplaceEvent::WouldDraw {
                    by,
                    except_first_in_draw_step,
                } = &r.event
                else {
                    continue;
                };
                let rref = ReplacementRef {
                    source,
                    def: o.def,
                    ability,
                };
                if applied.contains(&rref)
                    || !self.player_rel(*by, o.controller, p)
                    || (*except_first_in_draw_step && first_in_draw_step)
                {
                    continue;
                }
                if let Some(c) = &r.condition {
                    let ctx = self.replacement_ctx(rref, o.controller, p, applied);
                    if !self.condition(&ctx, c) {
                        continue;
                    }
                }
                out.push(rref);
            }
        }
        out
    }

    pub(crate) fn draw_task_step(&mut self, t: DrawTask) {
        if !self.in_game(t.player) {
            return;
        }
        let reps = self.draw_replacements(t.player, &t.applied);
        match reps.as_slice() {
            [] => self.draw(t.player),
            [r] => self.apply_draw_replacement(&t, *r),
            _ => {
                let player = t.player;
                let options = reps.iter().map(|r| (r.source, r.def, r.ability)).collect();
                self.tasks.push(Task::Draw(t));
                // CR 616.1: the affected player chooses one to apply.
                self.decide(player, DecisionKind::ChooseReplacement { options }, "616.1");
            }
        }
    }

    pub(crate) fn draw_task_answer(&mut self, t: &DrawTask, option: usize) {
        let reps = self.draw_replacements(t.player, &t.applied);
        if let Some(r) = reps.get(option) {
            self.apply_draw_replacement(t, *r);
        }
    }

    /// CR 614.6: the draw never happens; the replacement's effect does,
    /// carrying what has been applied so far (CR 616.1f).
    fn apply_draw_replacement(&mut self, t: &DrawTask, r: ReplacementRef) {
        let controller = self.obj(r.source).map_or(t.player, |o| o.controller);
        let ctx = self.replacement_ctx(r, controller, t.player, &t.applied);
        self.log(Event::Replaced {
            source: r.source,
            def: r.def,
            ability: r.ability,
        });
        self.tasks
            .push(Task::Effect(Box::new(EffectTask::new(ctx))));
    }

    // ---- entering the battlefield (CR 614.1c–d, 614.12) ------------------------

    /// Whether a permanent with definition `def` entering under `controller`'s
    /// control enters tapped: its own "enters tapped" or another permanent's
    /// (CR 614.12: an effect from another source doesn't apply to itself).
    pub(crate) fn enters_tapped(&self, def: DefId, controller: PlayerId) -> bool {
        let taps = |r: &ir::ReplacementAbility| {
            matches!(r.instead, Effect::EntersTapped) && r.condition.is_none()
        };
        self.replacements(def)
            .any(|(_, r)| matches!(&r.event, ReplaceEvent::WouldEnter(f) if f.this) && taps(r))
            || self.battlefield.iter().any(|&s| {
                self.obj(s).is_some_and(|o| {
                    self.replacements(o.def).any(|(_, r)| {
                        taps(r)
                            && matches!(&r.event, ReplaceEvent::WouldEnter(f)
                                if !f.this && def_matches(self, def, controller, f, o.controller))
                    })
                })
            })
    }

    /// A self-replacement that replaces this object entering ("If this would
    /// enter, … instead"): its ability index.
    fn enter_instead(&self, def: DefId) -> Option<u8> {
        self.replacements(def).find_map(|(i, r)| {
            (matches!(&r.event, ReplaceEvent::WouldEnter(f) if f.this)
                && !matches!(r.instead, Effect::EntersTapped))
            .then_some(i)
        })
    }

    /// If `id` entering the battlefield under `controller`'s control is
    /// replaced (and that replacement hasn't been applied yet), starts the
    /// replacement's effect and returns true: the move doesn't happen.
    pub(crate) fn replace_entering(
        &mut self,
        id: ObjectId,
        controller: PlayerId,
        applied: &[ReplacementRef],
    ) -> bool {
        if !self.pool.has_replacements() {
            return false;
        }
        let Some(o) = self.obj(id).copied() else {
            return false;
        };
        let Some(ability) = self.enter_instead(o.def) else {
            return false;
        };
        let r = ReplacementRef {
            source: id,
            def: o.def,
            ability,
        };
        if applied.contains(&r) {
            return false;
        }
        let mut ctx = self.replacement_ctx(r, controller, controller, applied);
        ctx.triggering_object = Some(id);
        self.log(Event::Replaced {
            source: id,
            def: o.def,
            ability,
        });
        self.tasks
            .push(Task::Effect(Box::new(EffectTask::new(ctx))));
        true
    }

    // ---- rule statics (M4 plan R5) ---------------------------------------------

    /// CR 614.10: whether `p` skips `step` (checked before it begins).
    pub(crate) fn skips_step(&self, p: PlayerId, step: Step) -> bool {
        self.statics().any(|(_, controller, e)| {
            matches!(e, StaticEffect::SkipStep { who, step: s }
                if step_is(*s, step) && self.player_rel(*who, controller, p))
        })
    }

    /// CR 502.3: permanents that "don't untap during [their controller's] untap step".
    pub(crate) fn untap_exempt(&self) -> SmallVec<[ObjectId; 4]> {
        let mut out = SmallVec::new();
        for (source, controller, e) in self.statics() {
            if let StaticEffect::DoesntUntap(f) = e {
                if f.this {
                    out.push(source);
                } else {
                    out.extend(
                        self.battlefield
                            .iter()
                            .copied()
                            .filter(|id| self.matches_filter(*id, f, controller, Some(source))),
                    );
                }
            }
        }
        out
    }
}
