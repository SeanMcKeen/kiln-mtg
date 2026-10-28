//! Casting spells (CR 601.2a–i) as a resumable procedure
//! (`docs/03 Architecture/Subsystems/Casting & Activating.md`, M3 plan B-D6).
//!
//! The announcements of CR 601.2b come first (modes, alternative cost, X,
//! hybrid and Phyrexian payment), then targets (601.2c), then mana payment
//! and the other costs (601.2g–h). Every choice is offered only while a
//! complete legal cast still exists, so the CR 601.2e legality check can't
//! fail and no rewind (CR 733) is needed.

use kiln_cards::ir::{Ability, ActivatedAbility, CostPart, Effect, Quantity, TargetSpec, Timing};
use kiln_types::{CardType, ManaCost, ManaSymbol, ObjectId, PlayerId};
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
    /// CR 702.33a: whether to pay kicker (after the cost is chosen).
    Kicker,
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
    /// A spell's cost (restricted mana may count, CR 106.6), not an ability's.
    spell: bool,
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
/// Marks "a loyalty ability of this permanent was activated this turn"
/// in `activated_this_turn` (CR 606.3).
const LOYALTY_USED: u8 = u8::MAX;

/// Whether an activated ability is a loyalty ability (CR 606.4).
fn is_loyalty(x: &ActivatedAbility) -> bool {
    x.cost.iter().any(|c| matches!(c, CostPart::Loyalty(_)))
}

/// A spell's kicker cost (CR 702.33a), if it has one.
fn kicker_cost(def: &CardDef) -> Option<&[CostPart]> {
    def.abilities.iter().find_map(|a| match a {
        Ability::Keyword(kiln_cards::ir::Keyword::Kicker(c)) => Some(c.as_slice()),
        _ => None,
    })
}

/// Adds cost parts (mana symbols and other parts) to a cost shape.
fn add_parts(shape: &mut CostShape, parts: &[CostPart]) {
    for p in parts {
        match p {
            CostPart::Mana(s) => {
                if let Ok(c) = s.parse::<ManaCost>() {
                    shape.symbols.extend(c.symbols().iter().cloned());
                }
            }
            other => shape.other.push(other.clone()),
        }
    }
}

/// CR 113.6b: whether an activated ability works in `zone` (the
/// battlefield unless it states a zone).
pub(crate) fn ability_works_in(x: &ActivatedAbility, zone: ZoneId) -> bool {
    match x.zone {
        None => zone == ZoneId::Battlefield,
        Some(kiln_cards::ir::ZoneName::Hand) => matches!(zone, ZoneId::Hand(_)),
        Some(_) => false,
    }
}

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
        // An Aura spell targets what it will enchant (CR 303.4a).
        None => def
            .spell()
            .map_or_else(|| def.enchant.as_slice(), |s| s.targets.as_slice()),
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
                spell: false,
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
        return Some(CostShape {
            symbols,
            other,
            spell: false,
        });
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
        CastCost::Dash => add(def.dash_cost()?, &mut symbols),
        CastCost::Flashback => add(def.flashback_cost()?, &mut symbols),
        // CR 702.138a: the mana cost plus the escape grant's extra cost
        // (added by the game, which knows the grant).
        CastCost::Escape => symbols.extend(def.mana_cost.as_ref()?.symbols().iter().cloned()),
        // Life equal to its mana value (X is 0, CR 107.3b), rather than its mana cost.
        CastCost::LifeInsteadOfMana => add(
            &[CostPart::PayLife(Quantity::Fixed(i64::from(
                def.mana_value(),
            )))],
            &mut symbols,
        ),
    }
    if let Some(s) = spell {
        add(&s.additional_costs, &mut symbols);
    }
    Some(CostShape {
        symbols,
        other,
        spell: true,
    })
}

/// Takes up to `cut` generic mana off `symbols`, leaving at least one mana
/// (CR 601.2f). X counts as no mana here, so an X cost may be reduced less
/// than it could be once X is chosen, never more.
fn reduce_keeping_one(symbols: &mut Vec<ManaSymbol>, cut: u32) {
    let total: u32 = symbols
        .iter()
        .map(|s| match s {
            ManaSymbol::Generic(n) => *n,
            ManaSymbol::Variable(_) => 0,
            _ => 1,
        })
        .sum();
    let mut left = cut.min(total.saturating_sub(1));
    for s in symbols.iter_mut() {
        if let ManaSymbol::Generic(n) = s {
            let c = (*n).min(left);
            *n -= c;
            left -= c;
        }
    }
    symbols.retain(|s| *s != ManaSymbol::Generic(0));
}

/// Whether non-mana cost parts use X ("pay X life").
fn cost_uses_x(parts: &[CostPart]) -> bool {
    parts.iter().any(|p| {
        matches!(
            p,
            CostPart::PayLife(Quantity::Expr(kiln_cards::ir::QuantityExpr::X))
        )
    })
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
        // Non-mana costs that use X are checked with this X.
        let with_x;
        let ctx = if ctx.x != x && cost_uses_x(&shape.other) {
            with_x = EffectCtx { x, ..ctx.clone() };
            &with_x
        } else {
            ctx
        };
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
        let pool = self.usable_pool(p, shape.spell.then_some(ctx.source));
        let sources = self.mana_sources_for(p, except, shape.spell.then_some(ctx.source));
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
        let pool = self
            .usable_pool(p, shape.spell.then_some(ctx.source))
            .total();
        let sources = u32::try_from(
            self.mana_sources_for(p, except, shape.spell.then_some(ctx.source))
                .len(),
        )
        .unwrap_or(u32::MAX);
        let cap = if cost_uses_x(&shape.other) {
            1_000
        } else {
            pool.saturating_add(sources.saturating_mul(16)).min(1_000)
        };
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

    /// The modes of a modal spell (`ability` `None`) or modal triggered
    /// ability whose targets can be chosen (CR 700.2a–b).
    pub(crate) fn ok_modes(
        &self,
        def: &CardDef,
        ability: Option<u8>,
        p: PlayerId,
        this: Option<ObjectId>,
    ) -> Vec<u8> {
        let targets = targets_of(def, ability);
        let masks = def.mode_masks(ability);
        let slots_ok = |mask: u32| {
            targets
                .iter()
                .enumerate()
                .all(|(i, t)| mask & (1 << i) == 0 || self.slot_ok(t, p, this, def))
        };
        (0..masks.len())
            .filter(|m| slots_ok(masks[*m]))
            .filter_map(|m| u8::try_from(m).ok())
            .collect()
    }

    /// Cost choices `p` can complete for this spell (CR 601.2b, 118.9). With
    /// no legal targets, only an overload cost remains (it has no targets,
    /// CR 702.96b).
    fn cost_options(&self, def: &CardDef, ctx: &EffectCtx, p: PlayerId) -> Vec<CastCost> {
        let targets_ok = self.spell_targets_ok(def, p);
        let alts = def.spell().map_or(0, |s| s.alternative_costs.len());
        // From a graveyard (before casting, or as it is cast): the mana cost
        // only with a permission; escape if granted.
        let in_graveyard = self
            .obj(ctx.source)
            .is_some_and(|o| matches!(o.zone, ZoneId::Graveyard(_)));
        let being_cast = self.graveyard_casts.iter().find(|(s, _)| *s == ctx.source);
        let from_graveyard = in_graveyard || being_cast.is_some();
        let permitted = self.cast_permitted(ctx.source, p) || being_cast.is_some_and(|(_, ok)| *ok);
        let mut candidates: Vec<CastCost> = Vec::new();
        // Bolas's Citadel: from the library, only life instead of mana.
        let from_library = self
            .obj(ctx.source)
            .is_some_and(|o| matches!(o.zone, ZoneId::Library(_)))
            || self.library_casts.contains(&ctx.source);
        if from_library {
            candidates.push(CastCost::LifeInsteadOfMana);
        } else if !from_graveyard || permitted {
            candidates.push(CastCost::Normal);
            candidates.extend(
                (0..alts)
                    .filter_map(|i| u8::try_from(i).ok())
                    .map(CastCost::Alternative),
            );
            if def.dash_cost().is_some() {
                candidates.push(CastCost::Dash);
            }
        }
        let escapable = if in_graveyard {
            self.escape_cost(ctx.source, p).is_some()
        } else {
            self.escaping.iter().any(|(s, _)| *s == ctx.source)
        };
        if from_graveyard && escapable {
            candidates.push(CastCost::Escape);
        }
        // CR 702.34a: flashback, from its owner's graveyard.
        let own_graveyard = self
            .obj(ctx.source)
            .is_some_and(|o| o.zone == ZoneId::Graveyard(p) || o.zone == ZoneId::Stack);
        if from_graveyard && own_graveyard && def.flashback_cost().is_some() {
            candidates.push(CastCost::Flashback);
        }
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
            let overload = matches!(c, CastCost::Alternative(i)
                if crate::effects::alt_effect(def, i).is_some());
            if !targets_ok && !overload {
                continue;
            }
            if let Some(shape) = self.spell_shape(def, None, c, ctx.source, p)
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
        if !self.cast_allowed(p, id) {
            return false;
        }
        // CR 117.1a, 702.8a: instants and flash any time the player has
        // priority; everything else at sorcery timing.
        if !(def.is(CardType::Instant) || def.keywords.has(Keywords::FLASH)) && !sorcery_timing {
            return false;
        }
        if let Some(spell) = def.spell()
            && let Effect::Modal { min, .. } = &spell.effect
            && self.ok_modes(def, None, p, None).len() < *min as usize
        {
            return false;
        }
        if let Some(spec) = &def.enchant
            && !self.slot_ok(spec, p, None, def)
        {
            return false;
        }
        let ctx = cast_ctx(def_id, def, None, id, p);
        !self.cost_options(def, &ctx, p).is_empty()
    }

    /// Whether every target slot of a non-modal spell has a legal target.
    fn spell_targets_ok(&self, def: &CardDef, p: PlayerId) -> bool {
        def.spell().is_none_or(|spell| {
            matches!(spell.effect, Effect::Modal { .. })
                || spell.targets.iter().all(|t| self.slot_ok(t, p, None, def))
        })
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
        if o.controller != p || !self.activation_allowed(p, source) {
            return false;
        }
        // `ability` indexes the object's current abilities (CR 613).
        let Some(&(r, _)) = self.abilities_of(source).get(usize::from(ability)) else {
            return false;
        };
        let (def_id, ability) = (r.def, r.ability);
        let def = self.def(def_id);
        let Some(x) = activated(def, ability) else {
            return false;
        };
        // CR 113.6b: it works only from its zone (the battlefield unless stated).
        if !ability_works_in(x, o.zone) {
            return false;
        }
        // CR 602.5d: "activate only as a sorcery".
        if x.timing == Timing::Sorcery && !sorcery_timing {
            return false;
        }
        // CR 602.5b: "activate only once each turn".
        if x.once_per_turn && self.activated_this_turn.contains(&(source, ability)) {
            return false;
        }
        // CR 606.3: a loyalty ability: sorcery timing, and only one per
        // permanent each turn.
        if is_loyalty(x)
            && (!sorcery_timing || self.activated_this_turn.contains(&(source, LOYALTY_USED)))
        {
            return false;
        }
        // CR 302.6: a creature's {T}/{Q} abilities need it under control since the turn began.
        if taps_source(def, Some(ability)) && self.tap_sick(source) {
            return false;
        }
        if !x.targets.iter().all(|t| self.slot_ok(t, p, None, def)) {
            return false;
        }
        let ctx = cast_ctx(def_id, def, Some(ability), source, p);
        // CR 602.5b: "Activate only if ...".
        if x.condition
            .as_ref()
            .is_some_and(|c| !self.condition(&ctx, c))
        {
            return false;
        }
        let except = taps_source(def, Some(ability)).then_some(source);
        self.spell_shape(def, Some(ability), CastCost::Normal, source, p)
            .is_some_and(|shape| self.shape_payable(p, &ctx, &shape, 0, &[], except))
    }

    /// CR 602.2a: put the ability on the stack and start the activation procedure.
    pub(crate) fn begin_activate(&mut self, p: PlayerId, source: ObjectId, ability: u8) {
        let Some(&(r, _)) = self.abilities_of(source).get(usize::from(ability)) else {
            return;
        };
        self.turn.priority = Some(p);
        self.stack
            .push(StackItem::new(source, r.def, Some(r.ability), p));
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
        let any_color = self.exile_permission(card).is_some_and(|(_, any)| any);
        let tax = self.command_zone_tax(card);
        let escape = self.escape_cost(card, p);
        let from_graveyard = self
            .obj(card)
            .is_some_and(|o| matches!(o.zone, ZoneId::Graveyard(_)));
        let permitted = self.cast_permitted(card, p);
        let from_library = self
            .obj(card)
            .is_some_and(|o| matches!(o.zone, ZoneId::Library(_)));
        let Some(spell) = self.move_object(card, ZoneId::Stack, true) else {
            return;
        };
        if from_library {
            self.library_casts.push(spell);
        }
        if from_graveyard {
            self.graveyard_casts.push((spell, permitted));
        }
        // Kept for the Cost stage, which may choose escape (CR 702.138a).
        if let Some(parts) = escape {
            self.escaping.push((spell, parts));
        }
        if let Some(tax) = tax {
            self.commander_cast(spell, tax);
        }
        // CR 601.2a: the player casting it controls it (it may be another's card).
        self.set_controller(spell, p);
        if any_color {
            self.any_color_spells.push(spell);
        }
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
        self.bump_layers();
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
        if !self.cast_allowed(p, id) {
            return false;
        }
        let def = self.def(def_id);
        // CR 305.9: lands can't be cast.
        if def.is(CardType::Land) {
            return false;
        }
        if let Some(spell) = def.spell() {
            if let Effect::Modal { min, .. } = &spell.effect {
                if self.ok_modes(def, None, p, None).len() < *min as usize {
                    return false;
                }
            } else if !spell.targets.iter().all(|t| self.slot_ok(t, p, None, def)) {
                return false;
            }
        }
        // An Aura spell targets what it will enchant (CR 303.4a).
        if let Some(spec) = &def.enchant
            && !self.slot_ok(spec, p, None, def)
        {
            return false;
        }
        let ctx = cast_ctx(def_id, def, None, id, p);
        // Cost increases and additional costs still apply (CR 601.2f).
        self.spell_shape(def, None, CastCost::WithoutPaying, id, p)
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

    /// The stage after modes: a triggered ability has no costs, so its
    /// targets come next (CR 603.3c-d).
    fn after_modes(&self, spell: ObjectId) -> CastStage {
        let trigger = self.item(spell).is_some_and(|i| {
            i.ability.is_some_and(|a| {
                matches!(
                    self.def(i.source_def).abilities.get(usize::from(a)),
                    Some(Ability::Triggered(_))
                )
            })
        });
        if trigger {
            CastStage::Targets
        } else {
            CastStage::Cost
        }
    }

    /// Strive: whether the total cost stays payable with one more target.
    fn another_target_payable(
        &self,
        def: &CardDef,
        item: &StackItem,
        p: PlayerId,
        ctx: &crate::effects::EffectCtx,
        except: Option<ObjectId>,
    ) -> bool {
        let Some(extra) = def
            .spell()
            .map(|s| s.per_extra_target.as_slice())
            .filter(|c| !c.is_empty())
        else {
            return true;
        };
        self.spell_shape(def, None, item.cost, item.object, item.controller)
            .map(|mut s| {
                add_parts(&mut s, extra);
                s
            })
            .is_some_and(|s| self.shape_payable(p, ctx, &s, item.x, &item.pays, except))
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
        let mut ctx = cast_ctx(def_id, def, item.ability, t.spell, t.caster);
        // CR 107.3a: the announced X applies to every cost that uses it.
        ctx.x = item.x;
        // A {T}/{Q} cost keeps the source out of the mana that pays the rest.
        let except = taps_source(def, item.ability).then_some(t.spell);
        match t.stage {
            CastStage::Modes => {
                // A triggered ability has no costs: after its modes, its targets (CR 603.3c-d).
                let (effect, next, rule) = match item.ability {
                    None => (def.spell().map(|s| &s.effect), CastStage::Cost, "700.2a"),
                    Some(i) => match def.abilities.get(usize::from(i)) {
                        Some(Ability::Triggered(ta)) => {
                            (Some(&ta.effect), CastStage::Targets, "700.2b")
                        }
                        // A modal activated ability: modes first, then costs (CR 700.2a, 602.2b).
                        Some(Ability::Activated(aa)) => {
                            (Some(&aa.effect), CastStage::Cost, "700.2a")
                        }
                        _ => (None, CastStage::Cost, "700.2a"),
                    },
                };
                let Some(Effect::Modal { min, max, .. }) = effect else {
                    return self.advance(t, next);
                };
                let chosen = item.modes.len();
                let mut modes: Vec<Option<u8>> = Vec::new();
                if chosen >= *min as usize {
                    modes.push(None);
                }
                if chosen < *max as usize {
                    modes.extend(
                        self.ok_modes(def, item.ability, t.caster, Some(t.spell))
                            .into_iter()
                            .filter(|m| !item.modes.contains(m))
                            .map(Some),
                    );
                }
                if modes.iter().all(Option::is_none) {
                    return self.advance(t, next);
                }
                self.tasks.push(Task::Cast(t));
                self.decide(
                    t.caster,
                    DecisionKind::ChooseMode {
                        spell: t.spell,
                        modes,
                    },
                    rule,
                );
            }
            CastStage::Cost => {
                // Abilities have only their activation cost; an effect that
                // casts a spell "without paying" fixed its cost already.
                if !item.is_spell() {
                    return self.advance(t, CastStage::X);
                }
                if item.cost == CastCost::WithoutPaying {
                    return self.advance(t, CastStage::Kicker);
                }
                let options = self.cost_options(def, &ctx, t.caster);
                debug_assert!(!options.is_empty(), "castability guarantees a cost");
                if let [only] = options.as_slice() {
                    let only = *only;
                    if let Some(i) = self.item_mut(t.spell) {
                        i.cost = only;
                    }
                    if only == CastCost::Flashback {
                        self.flashback.push(t.spell);
                    }
                    return self.advance(t, CastStage::Kicker);
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
            // CR 702.33a, 601.2b: kicker is announced with the other costs;
            // offered only if the total cost with it can be paid.
            CastStage::Kicker => {
                let kickable = kicker_cost(def).is_some()
                    && self
                        .spell_shape(def, None, item.cost, item.object, item.controller)
                        .map(|mut s| {
                            add_parts(&mut s, kicker_cost(def).unwrap_or(&[]));
                            s
                        })
                        .is_some_and(|s| self.shape_payable(t.caster, &ctx, &s, 0, &[], except));
                if !kickable {
                    return self.advance(t, CastStage::X);
                }
                self.tasks.push(Task::Cast(t));
                self.decide(
                    t.caster,
                    DecisionKind::Optional {
                        source: t.spell,
                        kind: crate::decision::OptionalKind::Kicker,
                    },
                    "702.33a",
                );
            }
            CastStage::X => {
                let Some(shape) =
                    self.spell_shape(def, item.ability, item.cost, item.object, item.controller)
                else {
                    return;
                };
                // CR 107.3a: X in the mana cost or in an additional cost
                // ("As an additional cost to cast this spell, pay X life").
                if !shape
                    .symbols
                    .iter()
                    .any(|s| matches!(s, ManaSymbol::Variable(_)))
                    && !cost_uses_x(&shape.other)
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
                let Some(shape) =
                    self.spell_shape(def, item.ability, item.cost, item.object, item.controller)
                else {
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
                // CR 702.96b: an overloaded spell has no targets.
                if let CastCost::Alternative(i) = item.cost
                    && item.is_spell()
                    && crate::effects::alt_effect(def, i).is_some()
                {
                    return self.advance(t, CastStage::Pay);
                }
                let slot = usize::from(t.index);
                let Some(spec) = targets_of(def, item.ability).get(slot) else {
                    return self.advance(t, CastStage::Pay);
                };
                let next_slot = CastTask {
                    index: t.index + 1,
                    ..t
                };
                // CR 601.2c: a modal spell requires only its chosen modes' targets.
                let masks = def.mode_masks(item.ability);
                let required = masks.is_empty()
                    || item
                        .modes
                        .iter()
                        .any(|m| masks[usize::from(*m)] & (1 << slot) != 0);
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
                let mut candidates: Vec<Target> = self
                    .legal_targets(&spec.what, t.caster, Some(t.spell), Some(def))
                    .into_iter()
                    .filter(|c| !chosen.contains(c))
                    .collect();
                // Strive: a target beyond the first must leave the cost payable.
                if item.is_spell()
                    && !item.targets.is_empty()
                    && !self.another_target_payable(def, &item, t.caster, &ctx, except)
                {
                    candidates.clear();
                }
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
                let spell = item.ability.is_none().then_some(t.spell);
                let plans = enumerate_plans(
                    &req,
                    &self.usable_pool(t.caster, spell),
                    &self.mana_sources_for(t.caster, except, spell),
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
                let Some(shape) =
                    self.spell_shape(def, item.ability, item.cost, item.object, item.controller)
                else {
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
                    if self.pool.tracks_spells() {
                        let colors = self.chars(t.spell).map_or(def.colors, |c| c.colors());
                        self.cast_this_turn.push(crate::state::CastRecord {
                            player: t.caster,
                            def: def_id,
                            object: t.spell,
                            colors,
                        });
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
                    if activated(def, ability).is_some_and(is_loyalty) {
                        self.activated_this_turn.push((t.spell, LOYALTY_USED));
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

    /// [`cost_shape`] for a spell, with mana spent as though it were any
    /// color when an effect allows it (Opposition Agent): every colored
    /// symbol is then payable by any mana.
    /// `caster`: who casts it (CR 601.2f: "spells you cast" means the
    /// caster's, even before the card is on the stack).
    fn spell_shape(
        &self,
        def: &CardDef,
        ability: Option<u8>,
        cost: CastCost,
        object: ObjectId,
        caster: PlayerId,
    ) -> Option<CostShape> {
        let mut shape = cost_shape(def, ability, cost)?;
        if ability.is_none() && cost == CastCost::Escape {
            let extra = self
                .escaping
                .iter()
                .find(|(s, _)| *s == object)
                .map(|(_, parts)| parts.clone())
                .or_else(|| {
                    let p = self.obj(object)?.owner;
                    self.escape_cost(object, p)
                })?;
            add_parts(&mut shape, &extra);
        }
        // "This ability costs {1} less to activate for each ...": generic mana only.
        if let Some(i) = ability
            && let Some(x) = activated(def, i)
            && let Some(q) = &x.cost_reduction
            && let Some(o) = self.obj(object)
        {
            let ctx = crate::effects::EffectCtx::new(
                crate::effects::AbilityRef {
                    def: o.def,
                    ability: i,
                },
                object,
                o.controller,
            );
            let mut left = self.amount(&ctx, q);
            for s in &mut shape.symbols {
                if let ManaSymbol::Generic(n) = s {
                    let cut = (*n).min(left);
                    *n -= cut;
                    left -= cut;
                }
            }
            shape.symbols.retain(|s| *s != ManaSymbol::Generic(0));
        }
        // Power Artifact: "cost {N} less to activate", leaving at least one mana.
        if ability.is_some() {
            let cut = self.activation_cost_reduction(object);
            if cut > 0 {
                reduce_keeping_one(&mut shape.symbols, cut);
            }
        }
        // Strive: "costs [cost] more for each target beyond the first" (CR 601.2f).
        if ability.is_none()
            && let Some(s) = def.spell()
            && !s.per_extra_target.is_empty()
            && let Some(item) = self
                .stack
                .iter()
                .rev()
                .find(|i| i.is_spell() && i.object == object)
        {
            for _ in 1..item.targets.len() {
                add_parts(&mut shape, &s.per_extra_target);
            }
        }
        // CR 702.33: a paid kicker is part of the total cost.
        if ability.is_none()
            && self.kicked.contains(&object)
            && let Some(k) = kicker_cost(def)
        {
            add_parts(&mut shape, k);
        }
        // CR 601.2f: cost increases apply whatever cost is paid (T24).
        if ability.is_none() {
            let more = self.cost_increase(object, caster);
            if more > 0 {
                shape.symbols.push(ManaSymbol::Generic(more));
            }
        }
        // CR 903.8: commander tax, an additional cost whatever cost is paid.
        if ability.is_none() {
            let tax = self.commander_tax(object);
            if tax > 0 {
                shape.symbols.push(ManaSymbol::Generic(tax));
            }
        }
        // CR 601.2f: cost reductions, after increases and additional costs;
        // they reduce only the generic part, never below zero.
        if ability.is_none() {
            let mut left = self.cost_reduction(object, caster);
            if let Some(o) = self.obj(object)
                && let Some((i, q)) = def.abilities.iter().enumerate().find_map(|(i, a)| match a {
                    Ability::Spell(s) => s.cost_reduction.as_ref().map(|q| (i, q)),
                    _ => None,
                })
                && let Ok(i) = u8::try_from(i)
            {
                let ctx = crate::effects::EffectCtx::new(
                    crate::effects::AbilityRef {
                        def: o.def,
                        ability: i,
                    },
                    object,
                    o.controller,
                );
                left += self.amount(&ctx, q);
            }
            for s in &mut shape.symbols {
                if let ManaSymbol::Generic(n) = s {
                    let cut = (*n).min(left);
                    *n -= cut;
                    left -= cut;
                }
            }
            shape.symbols.retain(|s| *s != ManaSymbol::Generic(0));
        }
        let any_color = ability.is_none()
            && (self.any_color_spells.contains(&object)
                || self.exile_permission(object).is_some_and(|(_, any)| any));
        if any_color {
            for s in &mut shape.symbols {
                if matches!(
                    s,
                    ManaSymbol::Colored(_)
                        | ManaSymbol::Hybrid(..)
                        | ManaSymbol::ColorlessHybrid(_)
                ) {
                    *s = ManaSymbol::Generic(1);
                }
            }
        }
        Some(shape)
    }

    /// The mana requirement and Phyrexian life of the chosen cost.
    pub(crate) fn cast_requirement(&self, def: &CardDef, item: &StackItem) -> (CostReq, i64) {
        self.spell_shape(def, item.ability, item.cost, item.object, item.controller)
            .map(|s| requirement(&s.symbols, item.x, &item.pays))
            .unwrap_or_default()
    }

    pub(crate) fn cast_answer(&mut self, t: CastTask, kind: DecisionKind, option: usize) {
        match kind {
            DecisionKind::ChooseMode { modes, .. } => match modes[option] {
                None => {
                    let next = self.after_modes(t.spell);
                    self.advance(t, next);
                }
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
                if options[option] == CastCost::Flashback {
                    self.flashback.push(t.spell);
                }
                self.advance(t, CastStage::Kicker);
            }
            DecisionKind::Optional {
                kind: crate::decision::OptionalKind::Kicker,
                ..
            } => {
                if option == 1 && !self.kicked.contains(&t.spell) {
                    self.kicked.push(t.spell);
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
                    let spell = self
                        .item(t.spell)
                        .is_some_and(|i| i.ability.is_none())
                        .then_some(t.spell);
                    self.execute_plan(t.caster, plan, spell);
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
        let spell = self.spell_being_cast(t.spell).then_some(t.spell);
        let pool = self.usable_pool(t.payer, spell);
        let sources = self.mana_sources_for(t.payer, t.except, spell);
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
                    let spell = self.spell_being_cast(t.spell).then_some(t.spell);
                    self.consume_restricted(t.payer, m, 1, spell);
                }
                self.tasks.push(Task::ManualPay(t));
            }
        }
    }
}

/// Why an action isn't available (FR-E10, the judge tool): the rule and the
/// reason, naming the card responsible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Explanation {
    pub cr: kiln_types::CrRef,
    pub reason: String,
}

fn explain(cr: &str, reason: impl Into<String>) -> Explanation {
    Explanation {
        cr: cr
            .parse()
            .unwrap_or_else(|e| panic!("engine CR literal {cr:?}: {e}")),
        reason: reason.into(),
    }
}

impl Game {
    /// Why `p` can't take `action` now: the checks the engine makes before
    /// offering it, in order, each with its rule (FR-E10). Empty when the
    /// action is offered now. Explanations come from the engine's own
    /// checks, never from a policy ([[Judge Tool UX]]).
    pub fn why_not(&self, p: PlayerId, action: &crate::decision::Action) -> Vec<Explanation> {
        use crate::decision::Action;
        let mut out = Vec::new();
        let name = |id: ObjectId| self.object_name(id).unwrap_or("that object").to_owned();
        match &self.pending {
            Some(d) if self.decider_for(d.player) == p => match &d.kind {
                DecisionKind::Priority { actions } if actions.contains(action) => return out,
                DecisionKind::Priority { .. } => {}
                _ => out.push(explain(
                    "117.1",
                    "the game is waiting for another kind of decision, not a priority action",
                )),
            },
            _ => out.push(explain("117.1", format!("{p} doesn't have priority now"))),
        }
        if self.split_second_on_stack() && !matches!(action, Action::ActivateMana { .. }) {
            out.push(explain(
                "702.61a",
                "a spell with split second is on the stack: only mana abilities can be activated",
            ));
        }
        let sorcery_timing =
            self.turn.active == p && self.turn.step.is_main() && self.stack.is_empty();
        let timing = |out: &mut Vec<Explanation>, cr: &str, what: &str| {
            let why = if self.turn.active != p {
                "it isn't this player's turn"
            } else if !self.turn.step.is_main() {
                "it isn't a main phase"
            } else {
                "the stack isn't empty"
            };
            out.push(explain(
                cr,
                format!("{what} only at sorcery timing, and {why}"),
            ));
        };
        match *action {
            Action::Pass => {}
            Action::PlayLand(id) => {
                let Some(o) = self.obj(id) else {
                    out.push(explain("305.1", "no such card"));
                    return out;
                };
                if !self.obj_def(id).is_some_and(|d| d.is(CardType::Land)) {
                    out.push(explain("305.1", format!("{} isn't a land", name(id))));
                }
                if o.zone != ZoneId::Hand(p) {
                    out.push(explain(
                        "305.1",
                        format!("{} isn't in {p}'s hand", name(id)),
                    ));
                }
                if !sorcery_timing {
                    timing(&mut out, "305.1", "a land can be played");
                }
                if self.player(p).lands_played >= 1 {
                    out.push(explain(
                        "305.2",
                        format!("{p} has already played a land this turn"),
                    ));
                }
            }
            Action::Cast(id) => {
                let Some(o) = self.obj(id) else {
                    out.push(explain("601.3", "no such card"));
                    return out;
                };
                let def_id = o.def;
                let def = self.def(def_id);
                if def.is(CardType::Land) {
                    out.push(explain(
                        "305.1",
                        format!("{} is a land: it's played, not cast", name(id)),
                    ));
                    return out;
                }
                let from_hand = o.zone == ZoneId::Hand(p);
                let from_command = o.zone == ZoneId::Command && self.is_commander(id);
                let permitted = self.cast_permitted(id, p)
                    || self.exile_permission(id).is_some_and(|(who, _)| who == p)
                    || self.escape_cost(id, p).is_some()
                    || self.top_card_visible(p) == Some(id);
                if !from_hand && !from_command && !permitted {
                    out.push(explain(
                        "601.3",
                        format!(
                            "{} is in {}, and nothing lets {p} cast it from there",
                            name(id),
                            crate::describe::zone(o.zone)
                        ),
                    ));
                }
                if let Some((source, what)) = self.cast_blocker(p, id) {
                    let by = source.map_or_else(String::new, |s| format!("{}: ", name(s)));
                    out.push(explain("101.2", format!("{by}{what}")));
                }
                if !(def.is(CardType::Instant) || def.keywords.has(Keywords::FLASH))
                    && !sorcery_timing
                {
                    timing(&mut out, "117.1a", "a noninstant spell can be cast");
                }
                if let Some(spell) = def.spell()
                    && let Effect::Modal { min, .. } = &spell.effect
                    && self.ok_modes(def, None, p, None).len() < *min as usize
                {
                    out.push(explain(
                        "700.2",
                        "not enough of its modes can be chosen (they lack legal targets)",
                    ));
                }
                if let Some(spec) = &def.enchant
                    && !self.slot_ok(spec, p, None, def)
                {
                    out.push(explain(
                        "303.4a",
                        "an Aura needs something to enchant, and nothing can be",
                    ));
                }
                if !self.spell_targets_ok(def, p) {
                    out.push(explain("601.2c", "it has no legal target"));
                }
                let ctx = cast_ctx(def_id, def, None, id, p);
                if out.is_empty() && self.cost_options(def, &ctx, p).is_empty() {
                    out.push(explain("601.2h", format!("{p} can't pay its total cost")));
                }
            }
            Action::Activate { source, ability } => {
                let Some(o) = self.obj(source) else {
                    out.push(explain("602.2", "no such object"));
                    return out;
                };
                if o.controller != p {
                    out.push(explain(
                        "602.2",
                        format!(
                            "only {}'s controller can activate its abilities",
                            name(source)
                        ),
                    ));
                }
                if let Some(by) = self.activation_blocker(p, source) {
                    out.push(explain(
                        "101.2",
                        format!("{}: its abilities can't be activated", name(by)),
                    ));
                }
                let Some(&(r, _)) = self.abilities_of(source).get(usize::from(ability)) else {
                    out.push(explain(
                        "602.2",
                        format!("{} has no ability {}", name(source), ability + 1),
                    ));
                    return out;
                };
                let def = self.def(r.def);
                let Some(x) = activated(def, r.ability) else {
                    out.push(explain("602.2", "that isn't an activated ability"));
                    return out;
                };
                if !ability_works_in(x, o.zone) {
                    out.push(explain(
                        "113.6b",
                        "the ability doesn't work from the zone its source is in",
                    ));
                }
                if x.timing == Timing::Sorcery && !sorcery_timing {
                    timing(&mut out, "602.5d", "it can be activated");
                }
                if x.once_per_turn && self.activated_this_turn.contains(&(source, r.ability)) {
                    out.push(explain(
                        "602.5b",
                        "it can be activated only once each turn, and already was",
                    ));
                }
                if is_loyalty(x) {
                    if !sorcery_timing {
                        timing(&mut out, "606.3", "a loyalty ability can be activated");
                    } else if self.activated_this_turn.contains(&(source, LOYALTY_USED)) {
                        out.push(explain(
                            "606.3",
                            "a loyalty ability of this permanent was already activated this turn",
                        ));
                    }
                }
                if taps_source(def, Some(r.ability)) && self.tap_sick(source) {
                    out.push(explain(
                        "302.6",
                        format!(
                            "{} hasn't been under its controller's control since the turn began",
                            name(source)
                        ),
                    ));
                }
                if !x.targets.iter().all(|t| self.slot_ok(t, p, None, def)) {
                    out.push(explain("601.2c", "it has no legal target"));
                }
                let ctx = cast_ctx(r.def, def, Some(r.ability), source, p);
                if x.condition
                    .as_ref()
                    .is_some_and(|c| !self.condition(&ctx, c))
                {
                    out.push(explain(
                        "602.5b",
                        "its \"activate only if\" condition isn't met",
                    ));
                }
                let except = taps_source(def, Some(r.ability)).then_some(source);
                if out.is_empty()
                    && !self
                        .spell_shape(def, Some(r.ability), CastCost::Normal, source, p)
                        .is_some_and(|shape| self.shape_payable(p, &ctx, &shape, 0, &[], except))
                {
                    out.push(explain("601.2h", format!("{p} can't pay its cost")));
                }
            }
            Action::ActivateMana { source, ability } => {
                if !self.can_activate_mana(p, source, ability) {
                    out.push(explain("605.3a", format!("{} can't be activated for mana now (its cost can't be paid, or it's restricted)", name(source))));
                }
            }
        }
        out
    }
}

impl Game {
    /// The action `text` names for `p`, so tools can ask [`Game::why_not`]
    /// by label: "Cast <card>", "Play <land>", "Activate <card> ability <n>"
    /// (as option labels read), or a bare card name (cast, or play a land).
    /// The card is looked for in `p`'s hand, the command zone, the
    /// battlefield, `p`'s graveyard and exile, in that order.
    pub fn action_for(&self, p: PlayerId, text: &str) -> Option<crate::decision::Action> {
        use crate::decision::Action;
        let text = text.trim();
        let find = |name: &str| -> Option<ObjectId> {
            let name = name.trim().to_lowercase();
            let pl = self.player(p);
            pl.hand
                .iter()
                .chain(&self.cmd.zone)
                .chain(&self.battlefield)
                .chain(&pl.graveyard)
                .chain(&self.exile)
                .copied()
                .find(|id| {
                    self.object_name(*id)
                        .is_some_and(|n| n.to_lowercase() == name)
                })
        };
        if let Some(rest) = text.strip_prefix("Activate ") {
            let (card, n) = rest.rsplit_once(" ability ")?;
            let n: u8 = n.trim().parse().ok()?;
            return Some(Action::Activate {
                source: find(card)?,
                ability: n.checked_sub(1)?,
            });
        }
        if let Some(card) = text.strip_prefix("Cast ") {
            return Some(Action::Cast(find(card)?));
        }
        if let Some(card) = text.strip_prefix("Play ") {
            return Some(Action::PlayLand(find(card)?));
        }
        let id = find(text)?;
        Some(if self.obj_def(id).is_some_and(|d| d.is(CardType::Land)) {
            Action::PlayLand(id)
        } else {
            Action::Cast(id)
        })
    }
}
