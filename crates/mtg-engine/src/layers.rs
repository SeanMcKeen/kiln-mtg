//! Continuous effects and the layer system (CR 611, 613;
//! `docs/03 Architecture/Subsystems/Continuous Effects & Layers.md`, M5 plan L1–L6).
//!
//! Characteristics are computed for the whole battlefield at once, because
//! one permanent's static abilities change others (and may remove each
//! other's abilities: dependencies, CR 613.8). The result is cached per
//! "layer epoch": every mutation that can change characteristics bumps the
//! epoch, and the next read recomputes. An invariant check compares the cache
//! with a fresh computation, so a missed bump shows up in fuzzing.
//!
//! When no continuous effect exists in the game (no stored effects and no
//! card in the pool with a layer static), characteristics are read straight
//! from the definitions: the fast path most games take.
//!
//! Layers applied: 4 (types), 6 (abilities), 7b (set P/T), 7c (modify P/T,
//! counters). Layer 1 (copy) and 2 (control) arrive in M5 5.3–5.4; 3, 5, 7a
//! and 7d have no supported effects yet.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use mtg_cards::ir::{Ability, BasePt, Condition, ObjectFilter, StaticEffect};
use mtg_core::{CardType, Color, Colors, ObjectId, PlayerId, Subtype, Supertype, TypeLine};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::card::{CardDef, DefId, Keywords};
use crate::effects::{AbilityRef, EffectCtx};
use crate::engine::Game;
use crate::state::ZoneId;

/// What a stored continuous effect does, by layer (CR 613.1).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum LayerEffect {
    /// Layer 1a: the object's copiable values become these (CR 707.2, 613.2a).
    Copy(Box<CopyValues>),
    /// Layer 2: `to` controls it; `from` controlled it before (CR 613.1b).
    /// The current controller is kept on the object itself.
    Control { to: PlayerId, from: PlayerId },
    /// Layer 4: it's this subtype in addition (amass, CR 701.47a).
    AddSubtype(String),
    /// Layer 6: it has this keyword (CR 613.1f).
    GrantKeyword(Box<mtg_cards::ir::Keyword>),
    /// Layer 7c: "+N/+M" (CR 613.4c).
    ModifyPt { power: i32, toughness: i32 },
}

/// Copiable values (CR 707.2): a definition plus the exceptions copy effects
/// added to it (CR 707.9), which are themselves copiable.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct CopyValues {
    pub def: DefId,
    pub types: SmallVec<[CardType; 1]>,
    pub subtypes: SmallVec<[Subtype; 1]>,
    /// Abilities granted by copy exceptions (CR 707.9a).
    pub abilities: SmallVec<[AbilityRef; 2]>,
    pub not_legendary: bool,
}

/// How long an effect lasts (CR 611.2a–b).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Duration {
    /// "Until end of turn": ends in the cleanup step (CR 514.2).
    EndOfTurn,
    /// For as long as the affected object exists (a copy effect, CR 707.2).
    Permanent,
}

/// A continuous effect from a resolved spell or ability (CR 611.2).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct ContinuousEffect {
    /// CR 613.7a: when the effect was created.
    pub timestamp: u64,
    /// CR 611.2c: the objects it affects, locked in as it was created.
    pub affected: SmallVec<[ObjectId; 4]>,
    pub effect: LayerEffect,
    pub duration: Duration,
}

/// An object's characteristics after the layers (CR 613).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Characteristics {
    /// The definition supplying the copiable values (layer 1, CR 707.2).
    pub def: DefId,
    /// Shared with the definition until an effect changes it (copy on write).
    pub type_line: Arc<TypeLine>,
    pub colors: Colors,
    /// Every ability the object has, as references into definitions.
    pub abilities: SmallVec<[AbilityRef; 6]>,
    pub keywords: Keywords,
    pub power: Option<i32>,
    pub toughness: Option<i32>,
    pub controller: PlayerId,
}

/// The battlefield's characteristics where the layers changed them.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LayerState {
    epoch: u64,
    /// Sorted by id; objects not listed have their printed values.
    changed: Vec<(ObjectId, Characteristics)>,
    /// Rule-modifying statics whose conditions hold (CR 613.11): source,
    /// its controller, the ability.
    pub(crate) rules: Vec<(ObjectId, PlayerId, AbilityRef)>,
}

/// The cached [`LayerState`]. Not part of the game state: a clone starts
/// empty and recomputes on first use. `computing` is set while the layers
/// are being applied: reads made then (a static ability's condition) see
/// printed values instead of recursing.
#[derive(Debug)]
pub(crate) struct LayerCache {
    state: Mutex<Option<Arc<LayerState>>>,
    computing: AtomicBool,
    /// The epoch whose computation changed nothing (reads then skip the lock).
    empty_at: AtomicU64,
    /// Printed type lines by definition, shared by every computation.
    printed: Mutex<Vec<Option<Arc<TypeLine>>>>,
}

impl Default for LayerCache {
    fn default() -> Self {
        Self {
            state: Mutex::default(),
            computing: AtomicBool::new(false),
            empty_at: AtomicU64::new(u64::MAX),
            printed: Mutex::default(),
        }
    }
}

impl Clone for LayerCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}

/// Characteristics as read: the printed values when no continuous effect
/// applies, otherwise computed.
#[derive(Debug)]
pub(crate) enum Chars<'g> {
    Printed {
        def_id: DefId,
        def: &'g CardDef,
        controller: PlayerId,
    },
    Computed(Arc<LayerState>, usize),
}

impl Chars<'_> {
    /// The definition supplying the object's copiable values (CR 707.2).
    pub fn def_id(&self) -> DefId {
        match self {
            Chars::Printed { def_id, .. } => *def_id,
            Chars::Computed(s, i) => s.changed[*i].1.def,
        }
    }

    fn computed(&self) -> Option<&Characteristics> {
        match self {
            Chars::Printed { .. } => None,
            Chars::Computed(s, i) => Some(&s.changed[*i].1),
        }
    }

    pub fn type_line(&self) -> &TypeLine {
        match self {
            Chars::Printed { def, .. } => &def.type_line,
            Chars::Computed(s, i) => &s.changed[*i].1.type_line,
        }
    }

    pub fn is(&self, t: CardType) -> bool {
        self.type_line().has_type(t)
    }

    pub fn colors(&self) -> Colors {
        match self {
            Chars::Printed { def, .. } => def.colors,
            Chars::Computed(..) => self.computed().map_or(Colors::COLORLESS, |c| c.colors),
        }
    }

    pub fn keywords(&self) -> Keywords {
        match self {
            Chars::Printed { def, .. } => def.keywords,
            Chars::Computed(..) => self.computed().map(|c| c.keywords).unwrap_or_default(),
        }
    }

    pub fn controller(&self) -> PlayerId {
        match self {
            Chars::Printed { controller, .. } => *controller,
            Chars::Computed(..) => self.computed().map_or(PlayerId(0), |c| c.controller),
        }
    }
}

/// CR 205.3i: the land types.
const LAND_TYPES: [&str; 17] = [
    "Cave",
    "Desert",
    "Forest",
    "Gate",
    "Island",
    "Lair",
    "Locus",
    "Mine",
    "Mountain",
    "Plains",
    "Planet",
    "Power-Plant",
    "Sphere",
    "Swamp",
    "Tower",
    "Town",
    "Urza's",
];

/// One continuous effect as the layers see it.
struct Live<'a> {
    timestamp: u64,
    /// The static ability generating it (`None` for a stored effect).
    source: Option<(ObjectId, AbilityRef)>,
    controller: PlayerId,
    condition: Option<&'a Condition>,
    /// Its parts, each in one layer (CR 613.6).
    parts: SmallVec<[Part<'a>; 2]>,
    /// Set once the effect starts applying (CR 613.6, 611.2c).
    set: Option<SmallVec<[ObjectId; 8]>>,
}

#[derive(Clone, Copy)]
enum Part<'a> {
    Static(&'a StaticEffect),
    Stored(&'a ContinuousEffect),
}

/// The layer a part applies in (CR 613.1, 613.4): 4, 6, 72 (7b) or 73 (7c).
fn layer_of(p: Part<'_>) -> Option<u8> {
    match p {
        Part::Stored(e) => match e.effect {
            LayerEffect::Copy(_) => Some(1),
            LayerEffect::Control { .. } => Some(2),
            LayerEffect::AddSubtype(_) => Some(4),
            LayerEffect::GrantKeyword(_) => Some(6),
            LayerEffect::ModifyPt { .. } => Some(73),
        },
        Part::Static(s) => match s {
            StaticEffect::LandType { .. }
            | StaticEffect::AddTypes { .. }
            | StaticEffect::RemoveTypes { .. } => Some(4),
            StaticEffect::LoseAllAbilities { .. } | StaticEffect::Grant { .. } => Some(6),
            StaticEffect::SetBasePt { .. } => Some(72),
            StaticEffect::Pump { .. } => Some(73),
            _ => None,
        },
    }
}

fn filter_of<'a>(p: Part<'a>) -> Option<&'a ObjectFilter> {
    match p {
        Part::Stored(_) => None,
        Part::Static(
            StaticEffect::LandType { affected, .. }
            | StaticEffect::AddTypes { affected, .. }
            | StaticEffect::RemoveTypes { affected, .. }
            | StaticEffect::LoseAllAbilities { affected }
            | StaticEffect::Grant { affected, .. }
            | StaticEffect::SetBasePt { affected, .. }
            | StaticEffect::Pump { affected, .. },
        ) => Some(affected),
        Part::Static(_) => None,
    }
}

/// Whether a static effect changes characteristics (so the layers handle it).
/// Rule-modifying statics the engine checks while it offers actions and
/// applies rules (CR 613.11), cached with the layers.
pub(crate) fn is_rule_static(e: &StaticEffect) -> bool {
    matches!(
        e,
        StaticEffect::CantCast { .. }
            | StaticEffect::CantActivate { .. }
            | StaticEffect::PlayerRule { .. }
            | StaticEffect::Rule(
                mtg_cards::ir::RuleMod::CreaturesEnteringDontTrigger
                    | mtg_cards::ir::RuleMod::ControlOpponentsWhileSearching
                    | mtg_cards::ir::RuleMod::MayPlayExiledWithThis { .. }
            )
    )
}

pub(crate) fn is_layer_static(e: &StaticEffect) -> bool {
    match e {
        StaticEffect::Multi(v) => v.iter().any(is_layer_static),
        e => layer_of(Part::Static(e)).is_some(),
    }
}

/// The working state of the computation: battlefield order, printed values.
#[derive(Clone)]
struct Board {
    ids: Vec<ObjectId>,
    chars: Vec<Characteristics>,
    touched: Vec<bool>,
    /// Board position by arena slot (`u32::MAX`: not on the board).
    slot: Vec<u32>,
}

impl Board {
    /// The board position of `id`.
    fn pos(&self, id: ObjectId) -> Option<usize> {
        let i = *self.slot.get(id.index as usize)? as usize;
        (self.ids.get(i) == Some(&id)).then_some(i)
    }
}

impl Game {
    /// A new timestamp (CR 613.7).
    pub(crate) fn next_timestamp(&mut self) -> u64 {
        self.timestamp += 1;
        self.timestamp
    }

    /// Characteristics may have changed: the next read recomputes.
    #[inline]
    pub(crate) fn bump_layers(&mut self) {
        self.layer_epoch += 1;
    }

    /// Whether the layers can change anything in this game.
    #[inline]
    pub(crate) fn layers_live(&self) -> bool {
        !self.effects.is_empty() || self.pool.has_layer_statics()
    }

    /// Object `id`'s characteristics (CR 613). Layers apply to permanents
    /// on the battlefield; elsewhere an object has its printed values.
    #[inline]
    pub(crate) fn chars(&self, id: ObjectId) -> Option<Chars<'_>> {
        let o = self.obj(id)?;
        let printed = || Chars::Printed {
            def_id: o.def,
            def: self.def(o.def),
            controller: o.controller,
        };
        if !self.layers_live()
            || o.zone != ZoneId::Battlefield
            || self.layer_cache.computing.load(Ordering::Relaxed)
            || self.layer_cache.empty_at.load(Ordering::Relaxed) == self.layer_epoch
        {
            return Some(printed());
        }
        let state = self.layer_state();
        match state.changed.binary_search_by_key(&id, |(k, _)| *k) {
            Ok(i) => Some(Chars::Computed(state, i)),
            Err(_) => Some(printed()),
        }
    }

    /// The cached layer state, recomputed if the epoch moved.
    fn layer_state(&self) -> Arc<LayerState> {
        let lock = || {
            self.layer_cache
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        };
        if let Some(s) = lock().as_ref()
            && s.epoch == self.layer_epoch
        {
            return Arc::clone(s);
        }
        // Not holding the lock: computing may read characteristics.
        self.layer_cache.computing.store(true, Ordering::Relaxed);
        let s = Arc::new(self.compute_layers());
        self.layer_cache.computing.store(false, Ordering::Relaxed);
        if s.changed.is_empty() {
            self.layer_cache
                .empty_at
                .store(self.layer_epoch, Ordering::Relaxed);
        }
        *lock() = Some(Arc::clone(&s));
        s
    }

    /// Invariant: the cache matches a fresh computation (a missed epoch bump
    /// would show as a difference).
    /// `None` if fresh; otherwise what differs.
    pub(crate) fn layer_cache_diff(&self) -> Option<String> {
        if !self.layers_live() {
            return None;
        }
        let cached = self
            .layer_cache
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()?;
        if cached.epoch != self.layer_epoch {
            return None;
        }
        self.layer_cache.computing.store(true, Ordering::Relaxed);
        let fresh = self.compute_layers();
        self.layer_cache.computing.store(false, Ordering::Relaxed);
        if *cached == fresh {
            return None;
        }
        let name = |id: ObjectId| {
            self.obj(id).map_or_else(
                || format!("{id}"),
                |o| format!("{id} {}", self.def(o.def).name),
            )
        };
        let mut out = Vec::new();
        for (id, c) in &cached.changed {
            match fresh.changed.iter().find(|(f, _)| f == id) {
                None => out.push(format!("{} only cached: {c:?}", name(*id))),
                Some((_, f)) if f != c => {
                    out.push(format!("{}: cached {c:?} fresh {f:?}", name(*id)));
                }
                Some(_) => {}
            }
        }
        for (id, f) in &fresh.changed {
            if !cached.changed.iter().any(|(c, _)| c == id) {
                out.push(format!("{} only fresh: {f:?}", name(*id)));
            }
        }
        Some(out.join("; "))
    }

    fn printed_chars(&self, id: ObjectId) -> Option<Characteristics> {
        let o = self.obj(id)?;
        let def = self.def(o.def);
        Some(Characteristics {
            def: o.def,
            type_line: self.printed_type_line(o.def),
            colors: def.colors,
            abilities: printed_refs(o.def, def),
            keywords: def.keywords,
            power: def.power,
            toughness: def.toughness,
            controller: o.controller,
        })
    }

    /// A definition's printed type line, shared (allocated once per game).
    fn printed_type_line(&self, def: DefId) -> Arc<TypeLine> {
        let mut cache = self
            .layer_cache
            .printed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let i = def.0 as usize;
        if cache.len() <= i {
            cache.resize(i + 1, None);
        }
        Arc::clone(cache[i].get_or_insert_with(|| Arc::new(self.def(def).type_line.clone())))
    }

    /// Layer 1a (CR 613.2a, 707.2): the object takes on copiable values.
    fn apply_copy(&self, c: &mut Characteristics, cv: &CopyValues) {
        let def = self.def(cv.def);
        c.def = cv.def;
        c.type_line = self.printed_type_line(cv.def);
        for t in &cv.types {
            if !c.type_line.card_types.contains(t) {
                Arc::make_mut(&mut c.type_line).card_types.push(*t);
            }
        }
        for st in &cv.subtypes {
            if !c.type_line.subtypes.contains(st) {
                Arc::make_mut(&mut c.type_line).subtypes.push(st.clone());
            }
        }
        if cv.not_legendary {
            Arc::make_mut(&mut c.type_line)
                .supertypes
                .retain(|s| *s != Supertype::Legendary);
        }
        c.colors = def.colors;
        c.abilities = printed_refs(cv.def, def);
        c.keywords = def.keywords;
        for r in &cv.abilities {
            c.abilities.push(*r);
            if let Some(Ability::Keyword(k)) = self.ability_at(*r)
                && let Some(bit) = Keywords::of_ir(k)
            {
                c.keywords = c.keywords.with(bit);
            }
        }
        c.power = def.power;
        c.toughness = def.toughness;
    }

    /// The copiable values of `id` (CR 707.2): its definition, or what its
    /// latest copy effect made it, exceptions included (CR 707.9b).
    pub(crate) fn copiable_values(&self, id: ObjectId) -> Option<CopyValues> {
        let o = self.obj(id)?;
        let latest = self
            .effects
            .iter()
            .filter(|e| e.affected.contains(&id))
            .filter_map(|e| match &e.effect {
                LayerEffect::Copy(cv) => Some((e.timestamp, cv)),
                _ => None,
            })
            .max_by_key(|(t, _)| *t);
        Some(match latest {
            Some((_, cv)) => (**cv).clone(),
            None => CopyValues {
                def: o.def,
                types: SmallVec::new(),
                subtypes: SmallVec::new(),
                abilities: SmallVec::new(),
                not_legendary: false,
            },
        })
    }

    /// A copy effect on `id` for as long as it exists (CR 707.2).
    pub(crate) fn add_copy_effect(&mut self, id: ObjectId, cv: CopyValues) {
        self.add_effect(
            SmallVec::from_elem(id, 1),
            LayerEffect::Copy(Box::new(cv)),
            Duration::Permanent,
        );
    }

    /// Applies the layers to the whole battlefield (CR 613.1–613.8).
    fn compute_layers(&self) -> LayerState {
        let empty = || LayerState {
            epoch: self.layer_epoch,
            changed: Vec::new(),
            rules: self.compute_rules(&[]),
        };
        // Nothing can apply: every permanent has its printed values
        // (counters are added on the printed path).
        if self.effects.is_empty() && !self.pool.has_layer_statics() {
            return empty();
        }
        let mut ids: Vec<ObjectId> = Vec::with_capacity(self.battlefield.len());
        let mut chars: Vec<Characteristics> = Vec::with_capacity(self.battlefield.len());
        let mut slot = vec![u32::MAX; self.objects.slot_count()];
        for &id in &self.battlefield {
            if let Some(c) = self.printed_chars(id) {
                if let Some(s) = slot.get_mut(id.index as usize) {
                    *s = u32::try_from(ids.len()).unwrap_or(u32::MAX);
                }
                ids.push(id);
                chars.push(c);
            }
        }
        let n = chars.len();
        let mut board = Board {
            ids,
            chars,
            touched: vec![false; n],
            slot,
        };
        // Layer 1a: copy effects, in timestamp order (CR 613.2a, 613.7).
        let mut copies: SmallVec<[&ContinuousEffect; 4]> = self
            .effects
            .iter()
            .filter(|e| matches!(e.effect, LayerEffect::Copy(_)))
            .collect();
        copies.sort_by_key(|e| e.timestamp);
        for e in &copies {
            let LayerEffect::Copy(cv) = &e.effect else {
                continue;
            };
            for id in &e.affected {
                if let Some(i) = board.pos(*id) {
                    self.apply_copy(&mut board.chars[i], cv);
                    board.touched[i] = true;
                }
            }
        }
        // Static abilities come from what the objects are after layer 1.
        let mut live = self.live_effects(&board);
        if live.is_empty() && copies.is_empty() {
            return empty();
        }
        // Layers in order; 7c also holds counters (CR 613.4c).
        for layer in [4u8, 6, 72, 73] {
            let mut done: SmallVec<[usize; 8]> = SmallVec::new();
            loop {
                let cands: SmallVec<[usize; 8]> = (0..live.len())
                    .filter(|i| !done.contains(i))
                    .filter(|&i| live[i].parts.iter().any(|p| layer_of(*p) == Some(layer)))
                    .filter(|&i| live[i].set.is_some() || self.exists(&board, &live[i]))
                    .collect();
                if cands.is_empty() {
                    break;
                }
                let pick = self.next_to_apply(&board, &live, &cands, layer);
                let set = match live[pick].set.clone() {
                    Some(s) => s,
                    None => self.affected_set(&board, &live[pick], layer),
                };
                self.apply_parts(&mut board, &live[pick], layer, &set);
                live[pick].set = Some(set);
                done.push(pick);
            }
            if layer == 73 {
                for (i, id) in board.ids.iter().enumerate() {
                    let d = self.counter_pt(*id);
                    if d != 0 {
                        let c = &mut board.chars[i];
                        c.power = c.power.map(|p| p + d);
                        c.toughness = c.toughness.map(|t| t + d);
                        board.touched[i] = true;
                    }
                }
            }
        }
        let mut changed: Vec<(ObjectId, Characteristics)> = board
            .ids
            .into_iter()
            .zip(board.chars)
            .zip(board.touched)
            .filter_map(|((id, c), t)| t.then_some((id, c)))
            .collect();
        changed.sort_by_key(|(id, _)| *id);
        let rules = self.compute_rules(&changed);
        LayerState {
            epoch: self.layer_epoch,
            changed,
            rules,
        }
    }

    /// The rule statics in effect: on permanents that have them after the
    /// layers, with their conditions holding.
    fn compute_rules(
        &self,
        changed: &[(ObjectId, Characteristics)],
    ) -> Vec<(ObjectId, PlayerId, AbilityRef)> {
        let mut out = Vec::new();
        if !self.pool.has_rule_statics() {
            return out;
        }
        for &id in &self.battlefield {
            let Some(o) = self.obj(id) else { continue };
            let computed = changed
                .binary_search_by_key(&id, |(k, _)| *k)
                .ok()
                .map(|i| &changed[i].1);
            let refs: SmallVec<[AbilityRef; 6]> = match computed {
                Some(c) => c.abilities.clone(),
                None => (0..self.def(o.def).printed_abilities().len())
                    .filter_map(|i| u8::try_from(i).ok())
                    .map(|ability| AbilityRef {
                        def: o.def,
                        ability,
                    })
                    .collect(),
            };
            let controller = computed.map_or(o.controller, |c| c.controller);
            for r in refs {
                let Some(Ability::Static(s)) = self.ability_at(r) else {
                    continue;
                };
                if is_rule_static(&s.effect)
                    && s.condition
                        .as_ref()
                        .is_none_or(|c| self.condition(&EffectCtx::new(r, id, controller), c))
                {
                    out.push((id, controller, r));
                }
            }
        }
        out
    }

    /// The rule statics in effect now (cached with the layers).
    pub(crate) fn rule_statics(&self) -> Option<Arc<LayerState>> {
        if !self.pool.has_rule_statics() || self.layer_cache.computing.load(Ordering::Relaxed) {
            return None;
        }
        Some(self.layer_state())
    }

    /// Every continuous effect: stored ones and the static abilities of
    /// permanents (CR 611.3), in timestamp order (CR 613.7).
    fn live_effects(&self, board: &Board) -> Vec<Live<'_>> {
        let mut out: Vec<Live<'_>> = Vec::new();
        for e in &self.effects {
            // Copy effects apply first (above); control is on the objects.
            if matches!(e.effect, LayerEffect::Copy(_) | LayerEffect::Control { .. }) {
                continue;
            }
            out.push(Live {
                timestamp: e.timestamp,
                source: None,
                controller: PlayerId(0),
                condition: None,
                parts: SmallVec::from_elem(Part::Stored(e), 1),
                set: Some(e.affected.iter().copied().collect()),
            });
        }
        if self.pool.has_layer_statics() {
            for (i, &id) in board.ids.iter().enumerate() {
                let Some(o) = self.obj(id) else { continue };
                let c = &board.chars[i];
                for &r in &c.abilities {
                    let Some(Ability::Static(s)) = self.ability_at(r) else {
                        continue;
                    };
                    if !is_layer_static(&s.effect) {
                        continue;
                    }
                    let parts: SmallVec<[Part<'_>; 2]> = match &s.effect {
                        StaticEffect::Multi(v) => v.iter().map(Part::Static).collect(),
                        e => SmallVec::from_elem(Part::Static(e), 1),
                    };
                    out.push(Live {
                        timestamp: o.timestamp,
                        source: Some((id, r)),
                        controller: c.controller,
                        condition: s.condition.as_ref(),
                        parts,
                        set: None,
                    });
                }
            }
        }
        out.sort_by_key(|l| l.timestamp);
        out
    }

    /// Whether an effect's static ability still exists: its source still has
    /// it, and its condition holds (CR 611.3a).
    fn exists(&self, board: &Board, l: &Live<'_>) -> bool {
        let Some((source, ability)) = l.source else {
            return true;
        };
        let has = board
            .pos(source)
            .is_some_and(|i| board.chars[i].abilities.contains(&ability));
        has && l.condition.is_none_or(|c| {
            let ctx = EffectCtx::new(ability, source, l.controller);
            self.condition(&ctx, c)
        })
    }

    /// The objects an effect's part in `layer` applies to, now.
    fn affected_set(&self, board: &Board, l: &Live<'_>, layer: u8) -> SmallVec<[ObjectId; 8]> {
        let Some(part) = l.parts.iter().find(|p| layer_of(**p) == Some(layer)) else {
            return SmallVec::new();
        };
        let Some(f) = filter_of(*part) else {
            return SmallVec::new();
        };
        let this = l.source.map(|(s, _)| s);
        let types = TypeWords::of(f);
        board
            .ids
            .iter()
            .enumerate()
            .filter(|(i, id)| {
                self.chars_match_with(&board.chars[*i], **id, f, &types, l.controller, this)
            })
            .map(|(_, id)| *id)
            .collect()
    }

    /// `matches_filter` against working characteristics `c` of object `id`.
    fn chars_match(
        &self,
        c: &Characteristics,
        id: ObjectId,
        f: &ObjectFilter,
        you: PlayerId,
        this: Option<ObjectId>,
    ) -> bool {
        self.chars_match_with(c, id, f, &TypeWords::of(f), you, this)
    }

    /// [`Game::chars_match`] with the filter's type words already resolved;
    /// cheap checks first.
    fn chars_match_with(
        &self,
        c: &Characteristics,
        id: ObjectId,
        f: &ObjectFilter,
        w: &TypeWords,
        you: PlayerId,
        this: Option<ObjectId>,
    ) -> bool {
        if (f.this && this != Some(id)) || (f.other && this == Some(id)) {
            return false;
        }
        if !self.player_rel(f.controller, you, c.controller) {
            return false;
        }
        let t = &c.type_line;
        if (!w.types.is_empty() && !w.types.iter().any(|x| t.card_types.contains(x)))
            || w.not_types.iter().any(|x| t.card_types.contains(x))
            || (!w.supertypes.is_empty() && !w.supertypes.iter().any(|x| t.supertypes.contains(x)))
            || w.not_supertypes.iter().any(|x| t.supertypes.contains(x))
        {
            return false;
        }
        if f.attached && !self.is_attached_to(this, id) {
            return false;
        }
        let has_sub = |w: &String| t.subtypes.iter().any(|s| s.0.eq_ignore_ascii_case(w));
        let def = self.def(c.def);
        (f.subtypes.is_empty() || f.subtypes.iter().any(has_sub))
            && !f.not_subtypes.iter().any(has_sub)
            && f.color.as_ref().is_none_or(|letters| {
                letters
                    .chars()
                    .filter_map(Color::from_letter)
                    .any(|col| c.colors.contains(col))
            })
            // Token status isn't copiable (CR 707.2): the object's own.
            && f.token.is_none_or(|tok| self.obj(id).is_some_and(|o| self.def(o.def).token == tok))
            && f.min_mana_value.is_none_or(|m| def.mana_value() >= m)
            && f.max_mana_value.is_none_or(|m| def.mana_value() <= m)
    }

    /// CR 613.8: the next effect to apply among `cands` (all in one layer):
    /// the earliest one that doesn't depend on another candidate. A
    /// dependency loop falls back to timestamp order (CR 613.8b).
    fn next_to_apply(&self, board: &Board, live: &[Live<'_>], cands: &[usize], layer: u8) -> usize {
        // Only type- and ability-changing effects can change another effect's
        // existence or what it applies to.
        if cands.len() < 2 || !matches!(layer, 4 | 6) {
            return cands[0];
        }
        for &a in cands {
            let depends = cands
                .iter()
                .any(|&b| b != a && self.depends_on(board, &live[a], &live[b], layer));
            if !depends {
                return a;
            }
        }
        cands[0]
    }

    /// CR 613.8a: whether applying `b` (in `layer`) would change whether `a`
    /// exists or what it applies to. `b` changes only the objects it
    /// affects, so only those are re-checked.
    fn depends_on(&self, board: &Board, a: &Live<'_>, b: &Live<'_>, layer: u8) -> bool {
        // Once started, an effect keeps applying to the same objects (CR 613.6).
        if a.set.is_some() {
            return false;
        }
        let set_b = b
            .set
            .clone()
            .unwrap_or_else(|| self.affected_set(board, b, layer));
        let filter_a = a
            .parts
            .iter()
            .find(|p| layer_of(**p) == Some(layer))
            .and_then(|p| filter_of(*p));
        let this_a = a.source.map(|(s, _)| s);
        for id in set_b {
            let Some(i) = board.pos(id) else {
                continue;
            };
            let before = &board.chars[i];
            let mut after = before.clone();
            for part in b.parts.iter().filter(|p| layer_of(**p) == Some(layer)) {
                self.apply_part(&mut after, *part);
            }
            if let Some((source, ability)) = a.source
                && source == id
                && before.abilities.contains(&ability) != after.abilities.contains(&ability)
            {
                return true;
            }
            if let Some(f) = filter_a
                && self.chars_match(before, id, f, a.controller, this_a)
                    != self.chars_match(&after, id, f, a.controller, this_a)
            {
                return true;
            }
        }
        false
    }

    /// Applies an effect's parts in `layer` to `set`.
    fn apply_parts(&self, board: &mut Board, l: &Live<'_>, layer: u8, set: &[ObjectId]) {
        for part in l.parts.iter().filter(|p| layer_of(**p) == Some(layer)) {
            for id in set {
                if let Some(i) = board.pos(*id) {
                    board.touched[i] = true;
                    self.apply_part(&mut board.chars[i], *part);
                }
            }
        }
    }

    /// Applies one effect part to one object's working characteristics.
    fn apply_part(&self, c: &mut Characteristics, part: Part<'_>) {
        match part {
            Part::Stored(e) => match &e.effect {
                LayerEffect::Copy(_) | LayerEffect::Control { .. } => {}
                LayerEffect::AddSubtype(st) => {
                    if !c.type_line.subtypes.iter().any(|x| x.0 == *st) {
                        Arc::make_mut(&mut c.type_line)
                            .subtypes
                            .push(Subtype(st.clone()));
                    }
                }
                LayerEffect::GrantKeyword(k) => {
                    if let Some(bit) = Keywords::of_ir(k) {
                        c.keywords = c.keywords.with(bit);
                    }
                }
                &LayerEffect::ModifyPt { power, toughness } => {
                    c.power = c.power.map(|p| p + power);
                    c.toughness = c.toughness.map(|t| t + toughness);
                }
            },
            Part::Static(s) => {
                let mana_value = self.def(c.def).mana_value();
                self.apply_static(c, s, mana_value);
            }
        }
    }

    fn apply_static(&self, c: &mut Characteristics, s: &StaticEffect, mana_value: u32) {
        match s {
            // CR 305.7: a land whose subtype is set to a basic land type
            // loses its other land types and its rules-text abilities, and
            // has the new type's mana ability.
            StaticEffect::LandType {
                land_type, replace, ..
            } => {
                if !c.type_line.has_type(CardType::Land) {
                    return;
                }
                if *replace {
                    Arc::make_mut(&mut c.type_line)
                        .subtypes
                        .retain(|s| !LAND_TYPES.contains(&s.0.as_str()));
                    c.abilities.clear();
                    c.keywords = Keywords::default();
                }
                if !c.type_line.subtypes.iter().any(|s| s.0 == *land_type) {
                    Arc::make_mut(&mut c.type_line)
                        .subtypes
                        .push(Subtype(land_type.clone()));
                    if let Some(r) = self.pool.intrinsic(land_type) {
                        c.abilities.push(r);
                    }
                } else if *replace && let Some(r) = self.pool.intrinsic(land_type) {
                    c.abilities.push(r);
                }
            }
            StaticEffect::AddTypes {
                types, subtypes, ..
            } => {
                for w in types {
                    if let Some(ct) = CardType::ALL
                        .iter()
                        .find(|ct| ct.word().eq_ignore_ascii_case(w))
                        && !c.type_line.card_types.contains(ct)
                    {
                        Arc::make_mut(&mut c.type_line).card_types.push(*ct);
                    }
                }
                for w in subtypes {
                    if !c.type_line.subtypes.iter().any(|s| s.0 == *w) {
                        Arc::make_mut(&mut c.type_line)
                            .subtypes
                            .push(Subtype(w.clone()));
                    }
                }
            }
            StaticEffect::RemoveTypes { types, .. } => {
                Arc::make_mut(&mut c.type_line)
                    .card_types
                    .retain(|t| !types.iter().any(|w| t.word().eq_ignore_ascii_case(w)));
            }
            StaticEffect::LoseAllAbilities { .. } => {
                c.abilities.clear();
                c.keywords = Keywords::default();
            }
            StaticEffect::Grant { keyword, .. } => {
                if let Some(bit) = Keywords::of_ir(keyword) {
                    c.keywords = c.keywords.with(bit);
                }
            }
            StaticEffect::SetBasePt {
                power, toughness, ..
            } => {
                let v = |b: &BasePt| match b {
                    BasePt::Fixed(n) => *n,
                    BasePt::ManaValue => i32::try_from(mana_value).unwrap_or(i32::MAX),
                };
                c.power = Some(v(power));
                c.toughness = Some(v(toughness));
            }
            StaticEffect::Pump {
                power, toughness, ..
            } => {
                c.power = c.power.map(|p| p + power);
                c.toughness = c.toughness.map(|t| t + toughness);
            }
            _ => {}
        }
    }

    /// Adds a continuous effect created by a resolving spell or ability.
    pub(crate) fn add_effect(
        &mut self,
        affected: SmallVec<[ObjectId; 4]>,
        effect: LayerEffect,
        duration: Duration,
    ) {
        let timestamp = self.next_timestamp();
        self.effects.push(ContinuousEffect {
            timestamp,
            affected,
            effect,
            duration,
        });
        self.bump_layers();
    }

    /// CR 514.2: "until end of turn" effects end in the cleanup step.
    pub(crate) fn end_turn_effects(&mut self) {
        self.end_effects(|e| e.duration == Duration::EndOfTurn);
    }

    /// Ends the effects `ending` selects (and effects whose objects are
    /// gone). CR 613.1b: a control effect ending returns control to the
    /// latest remaining control effect, or to the controller before it.
    pub(crate) fn end_effects(&mut self, ending: impl Fn(&ContinuousEffect) -> bool) {
        let reverting: Vec<(ObjectId, PlayerId, u64)> = self
            .effects
            .iter()
            .filter(|e| ending(e))
            .filter_map(|e| match e.effect {
                LayerEffect::Control { from, .. } => {
                    Some((*e.affected.first()?, from, e.timestamp))
                }
                _ => None,
            })
            .collect();
        let before = self.effects.len();
        // Also drop effects whose objects no longer exist.
        let objects = &self.objects;
        self.effects
            .retain(|e| !ending(e) && e.affected.iter().any(|id| objects.get(*id).is_some()));
        for (id, from, _) in reverting {
            let latest = self
                .effects
                .iter()
                .filter(|e| e.affected.contains(&id))
                .filter_map(|e| match e.effect {
                    LayerEffect::Control { to, .. } => Some((e.timestamp, to)),
                    _ => None,
                })
                .max_by_key(|(t, _)| *t)
                .map_or(from, |(_, to)| to);
            self.set_controller(id, latest);
        }
        if self.effects.len() != before {
            self.bump_layers();
        }
    }

    /// Sets an object's controller (layer 2's result, kept on the object);
    /// a permanent that changes controller is new to it (CR 302.6).
    pub(crate) fn set_controller(&mut self, id: ObjectId, p: PlayerId) {
        let turn = self.turn.number;
        if let Some(o) = self.objects.get_mut(id)
            && o.controller != p
        {
            o.controller = p;
            o.control_since = turn;
        }
        self.bump_layers();
    }

    /// CR 613.1b: `p` gains control of `id`.
    pub(crate) fn gain_control(&mut self, id: ObjectId, p: PlayerId, duration: Duration) {
        let Some(from) = self.obj(id).map(|o| o.controller) else {
            return;
        };
        self.add_effect(
            SmallVec::from_elem(id, 1),
            LayerEffect::Control { to: p, from },
            duration,
        );
        self.set_controller(id, p);
    }

    /// The ability an `AbilityRef` names.
    pub(crate) fn ability_at(&self, r: AbilityRef) -> Option<&Ability> {
        self.def(r.def).abilities.get(usize::from(r.ability))
    }

    /// An object's abilities after the layers, with their definitions.
    pub(crate) fn abilities_of(&self, id: ObjectId) -> SmallVec<[(AbilityRef, &Ability); 8]> {
        match self.chars(id) {
            None => SmallVec::new(),
            Some(Chars::Printed { def_id, def, .. }) => def
                .printed_abilities()
                .iter()
                .enumerate()
                .filter_map(|(i, a)| {
                    Some((
                        AbilityRef {
                            def: def_id,
                            ability: u8::try_from(i).ok()?,
                        },
                        a,
                    ))
                })
                .collect(),
            Some(Chars::Computed(s, i)) => s.changed[i]
                .1
                .abilities
                .iter()
                .filter_map(|r| Some((*r, self.ability_at(*r)?)))
                .collect(),
        }
    }

    /// Benchmarks: forces a full layer computation (as after any change)
    /// and reads every permanent's power.
    #[doc(hidden)]
    pub fn bench_characteristics(&mut self) -> i64 {
        self.bump_layers();
        self.battlefield
            .clone()
            .into_iter()
            .map(|id| i64::from(self.power(id)) + i64::from(self.toughness(id)))
            .sum()
    }

    /// A creature's current power and toughness (CR 613); `None` for a
    /// noncreature. For judge tools and tests.
    pub fn power_toughness(&self, id: ObjectId) -> Option<(i32, i32)> {
        self.is_creature(id)
            .then(|| (self.power(id), self.toughness(id)))
    }

    /// Effective power (0 if it has none).
    pub(crate) fn power(&self, id: ObjectId) -> i32 {
        self.pt(id, true)
    }

    pub(crate) fn toughness(&self, id: ObjectId) -> i32 {
        self.pt(id, false)
    }

    /// Power or toughness; printed values still get counters (layer 7c) on
    /// the battlefield.
    fn pt(&self, id: ObjectId, power: bool) -> i32 {
        match self.chars(id) {
            None => 0,
            Some(Chars::Printed { def, .. }) => {
                let printed = if power { def.power } else { def.toughness };
                let on_field = self.obj(id).is_some_and(|o| o.zone == ZoneId::Battlefield);
                printed.unwrap_or(0) + if on_field { self.counter_pt(id) } else { 0 }
            }
            Some(c @ Chars::Computed(..)) => c
                .computed()
                .and_then(|c| if power { c.power } else { c.toughness })
                .unwrap_or(0),
        }
    }

    #[inline]
    pub(crate) fn keywords(&self, id: ObjectId) -> Keywords {
        if !self.layers_live() {
            return self.obj_def(id).map(|d| d.keywords).unwrap_or_default();
        }
        self.chars(id).map(|c| c.keywords()).unwrap_or_default()
    }

    #[inline]
    pub(crate) fn is_creature(&self, id: ObjectId) -> bool {
        if !self.layers_live() {
            return self.obj_def(id).is_some_and(|d| d.is(CardType::Creature));
        }
        self.chars(id).is_some_and(|c| c.is(CardType::Creature))
    }
}

/// References to a definition's printed abilities.
fn printed_refs(def_id: DefId, def: &CardDef) -> SmallVec<[AbilityRef; 6]> {
    (0..def.printed_abilities().len())
        .filter_map(|i| u8::try_from(i).ok())
        .map(|ability| AbilityRef {
            def: def_id,
            ability,
        })
        .collect()
}

/// A filter's type and supertype words, resolved once per use.
struct TypeWords {
    types: SmallVec<[CardType; 2]>,
    not_types: SmallVec<[CardType; 2]>,
    supertypes: SmallVec<[Supertype; 1]>,
    not_supertypes: SmallVec<[Supertype; 1]>,
}

impl TypeWords {
    fn of(f: &ObjectFilter) -> Self {
        let types = |ws: &[String]| {
            ws.iter()
                .filter_map(|w| {
                    CardType::ALL
                        .iter()
                        .find(|t| t.word().eq_ignore_ascii_case(w))
                })
                .copied()
                .collect()
        };
        let supers = |ws: &[String]| {
            ws.iter()
                .filter_map(|w| {
                    Supertype::ALL
                        .iter()
                        .find(|t| t.word().eq_ignore_ascii_case(w))
                })
                .copied()
                .collect()
        };
        Self {
            types: types(&f.types),
            not_types: types(&f.not_types),
            supertypes: supers(&f.supertypes),
            not_supertypes: supers(&f.not_supertypes),
        }
    }
}
