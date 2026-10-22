//! Casting spells (CR 601.2a–i) as a resumable procedure
//! (`docs/03 Architecture/Subsystems/Casting & Activating.md`, M3 plan B-D6).
//!
//! The announcements of CR 601.2b come first (modes, alternative cost, X,
//! hybrid and Phyrexian payment), then targets (601.2c), then mana payment
//! and the other costs (601.2g–h). Every choice is offered only while a
//! complete legal cast still exists, so the CR 601.2e legality check can't
//! fail and no rewind (CR 733) is needed.

use mtg_cards::ir::{
    Ability, ActivatedAbility, CostPart, Effect, Quantity, SpellAbility, TargetSpec, Timing,
};
use mtg_core::{CardType, ManaCost, ManaSymbol, ObjectId, PlayerId};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::card::{CardDef, DefId, Keywords, ManaType};
use crate::decision::{CastCost, DecisionKind, ManaStep, SymbolPay};
use crate::effects::{AbilityRef, EffectCtx, EffectTask};
use crate::engine::{Game, Task};
use crate::events::Event;
use crate::mana::{CostReq, enumerate_plans, payable};
use crate::state::{StackItem, Target, ZoneId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum CastStage {
    /// CR 601.2b, 700.2a
    Modes,
    /// CR 601.2b, 118.9
    Cost,
    /// CR 601.2b, 107.3a
    X,
    /// CR 601.2b: hybrid and Phyrexian symbols.
    Symbols,
    /// CR 601.2c, `index` = target slot.
    Targets,
    /// CR 601.2f–h: mana.
    Pay,
    /// CR 601.2h: the other costs.
    PayOther,
    /// CR 601.2i
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct CastTask {
    pub spell: ObjectId,
    pub caster: PlayerId,
    pub stage: CastStage,
    pub index: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct ManualPayTask {
    pub spell: ObjectId,
    pub payer: PlayerId,
    pub remaining: CostReq,
    /// A source whose {T} is part of the cost being paid (not usable for mana).
    pub except: Option<ObjectId>,
}

/// The parts of a total cost before X and symbol choices (CR 601.2f).
struct CostShape {
    symbols: Vec<ManaSymbol>,
    other: Vec<CostPart>,
}

/// How each kind of hybrid or Phyrexian symbol may be paid (CR 107.4e–f).
pub(crate) fn symbol_options(s: &ManaSymbol) -> Vec<SymbolPay> {
    let m = |c| SymbolPay::Mana(ManaType::of_color(c));
    match s {
        ManaSymbol::Hybrid(a, b) => vec![m(*a), m(*b)],
        ManaSymbol::TwoHybrid(c) => vec![m(*c), SymbolPay::TwoGeneric],
        ManaSymbol::ColorlessHybrid(c) => vec![SymbolPay::Mana(ManaType::Colorless), m(*c)],
        ManaSymbol::Phyrexian(c) => vec![m(*c), SymbolPay::TwoLife],
        ManaSymbol::HybridPhyrexian(a, b) => vec![m(*a), m(*b), SymbolPay::TwoLife],
        _ => Vec::new(),
    }
}

fn is_open(s: &ManaSymbol) -> bool {
    !symbol_options(s).is_empty()
}

/// The mana requirement and life payment for fully chosen symbols.
fn requirement(symbols: &[ManaSymbol], x: u32, pays: &[SymbolPay]) -> (CostReq, i64) {
    let mut req = CostReq::default();
    let mut life = 0;
    let mut k = 0;
    for s in symbols {
        match s {
            ManaSymbol::Generic(n) => req.generic += n,
            ManaSymbol::Colored(c) => req.colored[ManaType::of_color(*c).index()] += 1,
            ManaSymbol::Colorless => req.colorless += 1,
            ManaSymbol::Variable(_) => req.generic += x,
            open if is_open(open) => {
                match pays.get(k) {
                    Some(SymbolPay::Mana(ManaType::Colorless)) => req.colorless += 1,
                    Some(SymbolPay::Mana(t)) => req.colored[t.index()] += 1,
                    Some(SymbolPay::TwoGeneric) => req.generic += 2,
                    Some(SymbolPay::TwoLife) => life += 2,
                    None => {}
                }
                k += 1;
            }
            // Snow and non-tournament symbols are rejected when the definition is built.
            _ => {}
        }
    }
    (req, life)
}

/// The activated ability `i` of a definition.
fn activated(def: &CardDef, i: u8) -> Option<&ActivatedAbility> {
    match def.abilities.get(usize::from(i))? {
        Ability::Activated(x) => Some(x),
        _ => None,
    }
}

/// The target slots of the spell or ability (CR 601.2c, 602.2b).
fn targets_of(def: &CardDef, ability: Option<u8>) -> &[TargetSpec] {
    match ability {
        Some(i) => match def.abilities.get(usize::from(i)) {
            Some(Ability::Activated(x)) => &x.targets,
            Some(Ability::Triggered(t)) => &t.targets,
            _ => &[],
        },
        None => def.spell().map_or(&[], |s| s.targets.as_slice()),
    }
}

/// Whether the cost includes {T} or {Q} of the source (CR 107.5–6), which
/// keeps the source out of the mana used to pay the rest.
fn taps_source(def: &CardDef, ability: Option<u8>) -> bool {
    ability.and_then(|i| activated(def, i)).is_some_and(|x| {
        x.cost
            .iter()
            .any(|c| matches!(c, CostPart::Tap | CostPart::Untap))
    })
}

/// The cost shape of the chosen cost: a spell's mana cost or alternative
/// cost plus additional costs, or an ability's activation cost. `None` if it
/// can't be paid at all (no mana cost, CR 202.1b; or no such alternative cost).
fn cost_shape(def: &CardDef, ability: Option<u8>, cost: CastCost) -> Option<CostShape> {
    if let Some(i) = ability {
        // A triggered ability has no cost to pay as it goes on the stack.
        if matches!(
            def.abilities.get(usize::from(i)),
            Some(Ability::Triggered(_))
        ) {
            return Some(CostShape {
                symbols: Vec::new(),
                other: Vec::new(),
            });
        }
        let mut symbols: Vec<ManaSymbol> = Vec::new();
        let mut other = Vec::new();
        for p in &activated(def, i)?.cost {
            match p {
                CostPart::Mana(s) => {
                    if let Ok(c) = s.parse::<ManaCost>() {
                        symbols.extend(c.symbols().iter().cloned());
                    }
                }
                other_part => other.push(other_part.clone()),
            }
        }
        return Some(CostShape { symbols, other });
    }
    let spell = def.spell();
    let mut symbols: Vec<ManaSymbol> = Vec::new();
    let mut other = Vec::new();
    let mut add = |parts: &[CostPart], symbols: &mut Vec<ManaSymbol>| {
        for p in parts {
            match p {
                CostPart::Mana(s) => {
                    if let Ok(c) = s.parse::<ManaCost>() {
                        symbols.extend(c.symbols().iter().cloned());
                    }
                }
                other_part => other.push(other_part.clone()),
            }
        }
    };
    match cost {
        CastCost::Normal => symbols.extend(def.mana_cost.as_ref()?.symbols().iter().cloned()),
        CastCost::Alternative(i) => {
            let alt = spell?.alternative_costs.get(usize::from(i))?;
            add(&alt.cost, &mut symbols);
        }
        // CR 118.9: no mana cost (and so X is 0, CR 107.3b); additional costs remain.
        CastCost::WithoutPaying => {}
    }
    if let Some(s) = spell {
        add(&s.additional_costs, &mut symbols);
    }
    Some(CostShape { symbols, other })
}

fn cast_ctx(
    def_id: DefId,
    def: &CardDef,
    ability: Option<u8>,
    source: ObjectId,
    p: PlayerId,
) -> EffectCtx {
    let ability = ability.unwrap_or_else(|| {
        def.abilities
            .iter()
            .position(|a| matches!(a, Ability::Spell(_)))
            .and_then(|i| u8::try_from(i).ok())
            .unwrap_or(0)
    });
    EffectCtx::new(
        AbilityRef {
            def: def_id,
            ability,
        },
        source,
        p,
    )
}

impl Game {
    // ---- payability -------------------------------------------------------

    /// Whether `p` can pay `shape` with this X and these symbol choices so far
    /// (the remaining open symbols chosen freely).
    #[allow(clippy::too_many_arguments)]
    fn shape_payable(
        &self,
        p: PlayerId,
        ctx: &EffectCtx,
        shape: &CostShape,
        x: u32,
        pays: &[SymbolPay],
        except: Option<ObjectId>,
    ) -> bool {
        if !self.can_pay(ctx, p, &shape.other) {
            return false;
        }
        let fixed_life: i64 = shape
            .other
            .iter()
            .filter_map(|c| match c {
                CostPart::PayLife(q) => Some(self.quantity(ctx, q).max(0)),
                _ => None,
            })
            .sum();
        let open: Vec<Vec<SymbolPay>> = shape
            .symbols
            .iter()
            .filter(|s| is_open(s))
            .skip(pays.len())
            .map(symbol_options)
            .collect();
        let pool = self.player(p).pool;
        let sources = self.mana_sources(p, except);
        let life = self.player(p).life;
        // Try every completion of the open symbols (a spell has only a few).
        let mut choice = vec![0usize; open.len()];
        loop {
            let mut all: SmallVec<[SymbolPay; 4]> = pays.iter().copied().collect();
            all.extend(choice.iter().zip(&open).map(|(i, o)| o[*i]));
            let (req, paid_life) = requirement(&shape.symbols, x, &all);
            // CR 119.4: life can be paid only up to the life total.
            let total_life = fixed_life + paid_life;
            if (total_life == 0 || life >= total_life) && payable(&req, &pool, &sources) {
                return true;
            }
            let mut i = 0;
            loop {
                if i == open.len() {
                    return false;
                }
                choice[i] += 1;
                if choice[i] < open[i].len() {
                    break;
                }
                choice[i] = 0;
                i += 1;
            }
        }
    }

    /// The largest X `p` could pay (CR 107.3a); payability is monotonic in X.
    fn max_x(
        &self,
        p: PlayerId,
        ctx: &EffectCtx,
        shape: &CostShape,
        except: Option<ObjectId>,
    ) -> u32 {
        let pool = self.player(p).pool.total();
        let sources = u32::try_from(self.mana_sources(p, except).len()).unwrap_or(u32::MAX);
        let cap = pool.saturating_add(sources.saturating_mul(16)).min(1_000);
        let mut x = 0;
        while x < cap && self.shape_payable(p, ctx, shape, x + 1, &[], except) {
            x += 1;
        }
        x
    }

    // ---- castability (exact, before offering) ------------------------------

    /// Whether a target slot can be satisfied ("up to N" slots always can).
    fn slot_ok(
        &self,
        spec: &TargetSpec,
        p: PlayerId,
        this: Option<ObjectId>,
        src: &CardDef,
    ) -> bool {
        spec.up_to.is_some()
            || !self
                .legal_targets(&spec.what, p, this, Some(src))
                .is_empty()
    }

    /// The modes of a modal spell whose targets can be chosen.
    fn ok_modes(
        &self,
        def: &CardDef,
        spell: &SpellAbility,
        p: PlayerId,
        this: Option<ObjectId>,
    ) -> Vec<u8> {
        let slots_ok = |mask: u32| {
            spell
                .targets
                .iter()
                .enumerate()
                .all(|(i, t)| mask & (1 << i) == 0 || self.slot_ok(t, p, this, def))
        };
        (0..def.mode_slots.len())
            .filter(|m| slots_ok(def.mode_slots[*m]))
            .filter_map(|m| u8::try_from(m).ok())
            .collect()
    }

    /// Cost choices `p` can complete for this spell (CR 601.2b, 118.9).
    fn cost_options(&self, def: &CardDef, ctx: &EffectCtx, p: PlayerId) -> Vec<CastCost> {
        let alts = def.spell().map_or(0, |s| s.alternative_costs.len());
        let candidates = std::iter::once(CastCost::Normal).chain(
            (0..alts)
                .filter_map(|i| u8::try_from(i).ok())
                .map(CastCost::Alternative),
        );
        let mut out = Vec::new();
        for c in candidates {
            if let CastCost::Alternative(i) = c {
                let cond = def
                    .spell()
                    .and_then(|s| s.alternative_costs.get(usize::from(i)))
                    .and_then(|a| a.condition.as_ref());
                if cond.is_some_and(|cond| !self.condition(ctx, cond)) {
                    continue;
                }
            }
            if let Some(shape) = cost_shape(def, None, c)
                && self.shape_payable(p, ctx, &shape, 0, &[], None)
            {
                out.push(c);
            }
        }
        out
    }

    /// Whether `p` may begin casting card `id` from hand right now, with a
    /// complete legal cast guaranteed to exist.
    pub(crate) fn can_cast(&self, p: PlayerId, id: ObjectId, sorcery_timing: bool) -> bool {
        let Some(def_id) = self.obj(id).map(|o| o.def) else {
            return false;
        };
        let def = self.def(def_id);
        // CR 117.1a, 702.8a: instants and flash any time the player has
        // priority; everything else at sorcery timing.
        if !(def.is(CardType::Instant) || def.keywords.has(Keywords::FLASH)) && !sorcery_timing {
            return false;
        }
        if let Some(spell) = def.spell() {
            if let Effect::Modal { min, .. } = &spell.effect {
                if self.ok_modes(def, spell, p, None).len() < *min as usize {
                    return false;
                }
            } else if !spell.targets.iter().all(|t| self.slot_ok(t, p, None, def)) {
                return false;
            }
        }
        let ctx = cast_ctx(def_id, def, None, id, p);
        !self.cost_options(def, &ctx, p).is_empty()
    }

    /// Whether `p` may activate non-mana ability `ability` of permanent
    /// `source` right now, with a complete legal activation guaranteed (CR 602.2, 602.5).
    pub(crate) fn can_activate(
        &self,
        p: PlayerId,
        source: ObjectId,
        ability: u8,
        sorcery_timing: bool,
    ) -> bool {
        let Some(o) = self.obj(source) else {
            return false;
        };
        // CR 602.2: only the controller of the source may activate it.
        if o.controller != p || o.zone != ZoneId::Battlefield {
            return false;
        }
        let def_id = o.def;
        let def = self.def(def_id);
        let Some(x) = activated(def, ability) else {
            return false;
        };
        // CR 602.5d: "activate only as a sorcery".
        if x.timing == Timing::Sorcery && !sorcery_timing {
            return false;
        }
        // CR 602.5b: "activate only once each turn".
        if x.once_per_turn && self.activated_this_turn.contains(&(source, ability)) {
            return false;
        }
        // CR 302.6: a creature's {T}/{Q} abilities need it under control since the turn began.
        if taps_source(def, Some(ability))
            && def.is(CardType::Creature)
            && self.is_summoning_sick(source)
        {
            return false;
        }
        if !x.targets.iter().all(|t| self.slot_ok(t, p, None, def)) {
            return false;
        }
        let ctx = cast_ctx(def_id, def, Some(ability), source, p);
        let except = taps_source(def, Some(ability)).then_some(source);
        cost_shape(def, Some(ability), CastCost::Normal)
            .is_some_and(|shape| self.shape_payable(p, &ctx, &shape, 0, &[], except))
    }

    /// CR 602.2a: put the ability on the stack and start the activation procedure.
    pub(crate) fn begin_activate(&mut self, p: PlayerId, source: ObjectId, ability: u8) {
        let Some(def) = self.obj(source).map(|o| o.def) else {
            return;
        };
        self.turn.priority = Some(p);
        self.stack
            .push(StackItem::new(source, def, Some(ability), p));
        self.tasks.push(Task::Cast(CastTask {
            spell: source,
            caster: p,
            stage: CastStage::Modes,
            index: 0,
        }));
    }

    // ---- the procedure ------------------------------------------------------

    /// CR 601.2a: move the card to the stack and start the procedure.
    pub(crate) fn begin_cast(&mut self, p: PlayerId, card: ObjectId) {
        self.turn.priority = Some(p);
        let Some(def) = self.obj(card).map(|o| o.def) else {
            return;
        };
        let Some(spell) = self.move_object(card, ZoneId::Stack, true) else {
            return;
        };
        self.stack.push(StackItem::new(spell, def, None, p));
        self.tasks.push(Task::Cast(CastTask {
            spell,
            caster: p,
            stage: CastStage::Modes,
            index: 0,
        }));
    }

    /// CR 608.2g, 707.12: an effect casts `card` (or a copy of a card) without
    /// paying its mana cost while it resolves. Returns the cast procedure to
    /// run before the effect continues, or `None` if no legal cast exists.
    pub(crate) fn cast_during_resolution(&mut self, p: PlayerId, card: ObjectId) -> Option<Task> {
        if !self.can_cast_free(p, card) {
            return None;
        }
        let def = self.obj(card)?.def;
        let spell = self.move_object(card, ZoneId::Stack, true)?;
        if let Some(o) = self.objects.get_mut(spell) {
            o.controller = p;
        }
        let mut item = StackItem::new(spell, def, None, p);
        item.cost = CastCost::WithoutPaying;
        self.stack.push(item);
        Some(Task::Cast(CastTask {
            spell,
            caster: p,
            stage: CastStage::Modes,
            index: 0,
        }))
    }

    /// Whether `p` could cast `id` without paying its mana cost right now
    /// (timing permissions don't apply during resolution, CR 608.2g).
    fn can_cast_free(&self, p: PlayerId, id: ObjectId) -> bool {
        let Some(def_id) = self.obj(id).map(|o| o.def) else {
            return false;
        };
        let def = self.def(def_id);
        // CR 305.9: lands can't be cast.
        if def.is(CardType::Land) {
            return false;
        }
        if let Some(spell) = def.spell() {
            if let Effect::Modal { min, .. } = &spell.effect {
                if self.ok_modes(def, spell, p, None).len() < *min as usize {
                    return false;
                }
            } else if !spell.targets.iter().all(|t| self.slot_ok(t, p, None, def)) {
                return false;
            }
        }
        let ctx = cast_ctx(def_id, def, None, id, p);
        cost_shape(def, None, CastCost::WithoutPaying)
            .is_some_and(|shape| self.shape_payable(p, &ctx, &shape, 0, &[], None))
    }

    /// The stack item being cast or activated: the topmost one for this
    /// object (an ability's object is its source, which may have other
    /// abilities lower on the stack).
    fn item(&self, spell: ObjectId) -> Option<&StackItem> {
        self.stack.iter().rev().find(|s| s.object == spell)
    }

    fn item_mut(&mut self, spell: ObjectId) -> Option<&mut StackItem> {
        self.stack.iter_mut().rev().find(|s| s.object == spell)
    }

    fn advance(&mut self, t: CastTask, stage: CastStage) {
        self.tasks.push(Task::Cast(CastTask {
            stage,
            index: 0,
            ..t
        }));
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn cast_step(&mut self, t: CastTask) {
        let Some(item) = self.item(t.spell).cloned() else {
            return;
        };
        let def_id = item.source_def;
        let pool = std::sync::Arc::clone(&self.pool);
        let def = pool.get(def_id);
        let ctx = cast_ctx(def_id, def, item.ability, t.spell, t.caster);
        // A {T}/{Q} cost keeps the source out of the mana that pays the rest.
        let except = taps_source(def, item.ability).then_some(t.spell);
        match t.stage {
            CastStage::Modes => {
                let Some(spell) = def.spell().filter(|_| item.is_spell()) else {
                    return self.advance(t, CastStage::Cost);
                };
                let Effect::Modal { min, max, .. } = &spell.effect else {
                    return self.advance(t, CastStage::Cost);
                };
                let chosen = item.modes.len();
                let mut modes: Vec<Option<u8>> = Vec::new();
                if chosen >= *min as usize {
                    modes.push(None);
                }
                if chosen < *max as usize {
                    modes.extend(
                        self.ok_modes(def, spell, t.caster, Some(t.spell))
                            .into_iter()
                            .filter(|m| !item.modes.contains(m))
                            .map(Some),
                    );
                }
                if modes.iter().all(Option::is_none) {
                    return self.advance(t, CastStage::Cost);
                }
                self.tasks.push(Task::Cast(t));
                self.decide(
                    t.caster,
                    DecisionKind::ChooseMode {
                        spell: t.spell,
                        modes,
                    },
                    "700.2a",
                );
            }
            CastStage::Cost => {
                // Abilities have only their activation cost; an effect that
                // casts a spell "without paying" fixed its cost already.
                if !item.is_spell() || item.cost == CastCost::WithoutPaying {
                    return self.advance(t, CastStage::X);
                }
                let options = self.cost_options(def, &ctx, t.caster);
                debug_assert!(!options.is_empty(), "castability guarantees a cost");
                if let [only] = options.as_slice() {
                    let only = *only;
                    if let Some(i) = self.item_mut(t.spell) {
                        i.cost = only;
                    }
                    return self.advance(t, CastStage::X);
                }
                self.tasks.push(Task::Cast(t));
                self.decide(
                    t.caster,
                    DecisionKind::ChooseCost {
                        spell: t.spell,
                        options,
                    },
                    "601.2b",
                );
            }
            CastStage::X => {
                let Some(shape) = cost_shape(def, item.ability, item.cost) else {
                    return;
                };
                if !shape
                    .symbols
                    .iter()
                    .any(|s| matches!(s, ManaSymbol::Variable(_)))
                {
                    return self.advance(t, CastStage::Symbols);
                }
                let max = self.max_x(t.caster, &ctx, &shape, except);
                self.tasks.push(Task::Cast(t));
                self.decide(
                    t.caster,
                    DecisionKind::ChooseNumber {
                        source: t.spell,
                        min: 0,
                        max: i64::from(max),
                    },
                    "107.3a",
                );
            }
            CastStage::Symbols => {
                let Some(shape) = cost_shape(def, item.ability, item.cost) else {
                    return;
                };
                let Some(symbol) = shape
                    .symbols
                    .iter()
                    .filter(|s| is_open(s))
                    .nth(item.pays.len())
                    .cloned()
                else {
                    return self.advance(t, CastStage::Targets);
                };
                let options: Vec<SymbolPay> = symbol_options(&symbol)
                    .into_iter()
                    .filter(|o| {
                        let mut pays = item.pays.to_vec();
                        pays.push(*o);
                        self.shape_payable(t.caster, &ctx, &shape, item.x, &pays, except)
                    })
                    .collect();
                debug_assert!(!options.is_empty(), "a payable cost has a payable choice");
                if let [only] = options.as_slice() {
                    let only = *only;
                    if let Some(i) = self.item_mut(t.spell) {
                        i.pays.push(only);
                    }
                    self.tasks.push(Task::Cast(t));
                    return;
                }
                self.tasks.push(Task::Cast(t));
                self.decide(
                    t.caster,
                    DecisionKind::ChooseSymbolPay {
                        spell: t.spell,
                        symbol,
                        options,
                    },
                    "601.2b",
                );
            }
            CastStage::Targets => {
                let slot = usize::from(t.index);
                let Some(spec) = targets_of(def, item.ability).get(slot) else {
                    return self.advance(t, CastStage::Pay);
                };
                let next_slot = CastTask {
                    index: t.index + 1,
                    ..t
                };
                // CR 601.2c: a modal spell requires only its chosen modes' targets.
                let required = def.mode_slots.is_empty()
                    || !item.is_spell()
                    || item
                        .modes
                        .iter()
                        .any(|m| def.mode_slots[usize::from(*m)] & (1 << slot) != 0);
                if !required {
                    self.tasks.push(Task::Cast(next_slot));
                    return;
                }
                let chosen: Vec<Target> = item
                    .targets
                    .iter()
                    .zip(&item.slots)
                    .filter(|(_, s)| usize::from(**s) == slot)
                    .map(|(t, _)| *t)
                    .collect();
                let max = spec.up_to.unwrap_or(1) as usize;
                let min = usize::from(spec.up_to.is_none());
                // CR 115.3: the same target can't be chosen twice for one "target".
                let candidates: Vec<Target> = self
                    .legal_targets(&spec.what, t.caster, Some(t.spell), Some(def))
                    .into_iter()
                    .filter(|c| !chosen.contains(c))
                    .collect();
                if chosen.len() >= max || (chosen.len() >= min && candidates.is_empty()) {
                    self.tasks.push(Task::Cast(next_slot));
                    return;
                }
                let mut targets: Vec<Option<Target>> = Vec::new();
                if chosen.len() >= min {
                    targets.push(None);
                }
                targets.extend(candidates.into_iter().map(Some));
                self.tasks.push(Task::Cast(t));
                self.decide(
                    t.caster,
                    DecisionKind::ChooseTarget {
                        spell: t.spell,
                        slot: t.index,
                        targets,
                    },
                    "601.2c",
                );
            }
            CastStage::Pay => {
                // CR 601.2f: the total cost; 601.2g–h: activate mana abilities and pay.
                let req = self.cast_requirement(def, &item).0;
                if req.is_paid() {
                    return self.advance(t, CastStage::PayOther);
                }
                let plans = enumerate_plans(
                    &req,
                    &self.player(t.caster).pool,
                    &self.mana_sources(t.caster, except),
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
            CastStage::PayOther => {
                self.advance(t, CastStage::Done);
                let Some(shape) = cost_shape(def, item.ability, item.cost) else {
                    return;
                };
                let life = self.cast_requirement(def, &item).1;
                let mut parts = shape.other;
                if life > 0 {
                    parts.push(CostPart::PayLife(Quantity::Fixed(life)));
                }
                if !parts.is_empty() {
                    self.tasks.push(Task::Effect(Box::new(EffectTask::paying(
                        ctx, t.caster, &parts,
                    ))));
                }
            }
            // A triggered ability is simply on the stack now (CR 603.3d).
            CastStage::Done if item.trigger.is_some() => self.became_targets(&item, t.caster),
            CastStage::Done => match item.ability {
                // CR 601.2i: the spell becomes cast.
                None => {
                    if self.pool.has_triggers() {
                        self.cast_this_turn.push((t.caster, def_id));
                    }
                    self.log(Event::SpellCast {
                        player: t.caster,
                        object: t.spell,
                        def: def_id,
                    });
                    self.became_targets(&item, t.caster);
                }
                // CR 602.2b (applying 601.2i): the ability becomes activated.
                Some(ability) => {
                    if activated(def, ability).is_some_and(|x| x.once_per_turn) {
                        self.activated_this_turn.push((t.spell, ability));
                    }
                    self.log(Event::AbilityActivated {
                        player: t.caster,
                        source: t.spell,
                        def: def_id,
                        ability,
                    });
                    self.became_targets(&item, t.caster);
                }
            },
        }
    }

    /// Each permanent a spell or ability targets "becomes the target" once
    /// (CR 115.1, 702.21a); only logged when something could trigger.
    fn became_targets(&mut self, item: &StackItem, controller: PlayerId) {
        if !self.pool.has_triggers() {
            return;
        }
        let mut seen: SmallVec<[ObjectId; 4]> = SmallVec::new();
        for t in &item.targets {
            if let Target::Object(id) = *t
                && !seen.contains(&id)
                && self.obj(id).is_some_and(|o| o.zone == ZoneId::Battlefield)
            {
                seen.push(id);
                self.log(Event::BecameTarget {
                    target: id,
                    source: item.object,
                    ability: item.ability,
                    controller,
                });
            }
        }
    }

    /// The mana requirement and Phyrexian life of the chosen cost.
    pub(crate) fn cast_requirement(&self, def: &CardDef, item: &StackItem) -> (CostReq, i64) {
        cost_shape(def, item.ability, item.cost)
            .map(|s| requirement(&s.symbols, item.x, &item.pays))
            .unwrap_or_default()
    }

    pub(crate) fn cast_answer(&mut self, t: CastTask, kind: DecisionKind, option: usize) {
        match kind {
            DecisionKind::ChooseMode { modes, .. } => match modes[option] {
                None => self.advance(t, CastStage::Cost),
                Some(m) => {
                    if let Some(i) = self.item_mut(t.spell) {
                        i.modes.push(m);
                        i.modes.sort_unstable();
                    }
                    self.tasks.push(Task::Cast(t));
                }
            },
            DecisionKind::ChooseCost { options, .. } => {
                if let Some(i) = self.item_mut(t.spell) {
                    i.cost = options[option];
                }
                self.advance(t, CastStage::X);
            }
            DecisionKind::ChooseNumber { min, .. } => {
                let x = u32::try_from(min + i64::try_from(option).unwrap_or(0)).unwrap_or(0);
                if let Some(i) = self.item_mut(t.spell) {
                    i.x = x;
                }
                self.advance(t, CastStage::Symbols);
            }
            DecisionKind::ChooseSymbolPay { options, .. } => {
                if let Some(i) = self.item_mut(t.spell) {
                    i.pays.push(options[option]);
                }
                self.tasks.push(Task::Cast(t));
            }
            DecisionKind::ChooseTarget { targets, slot, .. } => match targets[option] {
                None => self.tasks.push(Task::Cast(CastTask {
                    index: t.index + 1,
                    ..t
                })),
                Some(target) => {
                    if let Some(i) = self.item_mut(t.spell) {
                        i.targets.push(target);
                        i.slots.push(slot);
                    }
                    self.tasks.push(Task::Cast(t));
                }
            },
            DecisionKind::PayMana { plans, .. } => {
                self.advance(t, CastStage::PayOther);
                if let Some(plan) = plans.get(option) {
                    self.execute_plan(t.caster, plan);
                } else if let Some(item) = self.item(t.spell) {
                    let def = self.def(item.source_def);
                    let remaining = self.cast_requirement(def, item).0;
                    let except = taps_source(def, item.ability).then_some(t.spell);
                    self.tasks.push(Task::ManualPay(ManualPayTask {
                        spell: t.spell,
                        payer: t.caster,
                        remaining,
                        except,
                    }));
                }
            }
            other => unreachable!("cast procedure got {other:?}"),
        }
    }

    /// Manual payment steps that keep the payment completable.
    pub(crate) fn manual_steps(&self, t: &ManualPayTask) -> Vec<ManaStep> {
        let pool = self.player(t.payer).pool;
        let sources = self.mana_sources(t.payer, t.except);
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
            let rest: Vec<_> = sources
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, x)| x.clone())
                .collect();
            for (ability, bags) in &s.abilities {
                for bag in bags {
                    let mut p2 = pool;
                    for m in ManaType::ALL {
                        p2.add(m, u32::from(bag[m.index()]));
                    }
                    if payable(&t.remaining, &p2, &rest) {
                        steps.push(ManaStep::Activate {
                            source: s.object,
                            ability: *ability,
                            bag: *bag,
                        });
                    }
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
            ManaStep::Activate {
                source,
                ability,
                bag,
            } => {
                self.tasks.push(Task::ManualPay(t));
                self.activate_mana(source, ability, Some(bag));
            }
            ManaStep::Spend(m) => {
                if t.remaining.pay_one(m) {
                    self.player_mut(t.payer).pool.0[m.index()] -= 1;
                }
                self.tasks.push(Task::ManualPay(t));
            }
        }
    }
}
