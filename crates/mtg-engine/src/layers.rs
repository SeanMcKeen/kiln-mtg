//! Continuous effects and the layer system (CR 611, 613;
//! `docs/03 Architecture/Subsystems/Continuous Effects & Layers.md`, M5 plan L1–L6).
//!
//! An object's characteristics are computed: its copiable values (its
//! definition, CR 707.2) transformed by the continuous effects that apply to
//! it, layer by layer (CR 613.1). Effects created by resolving spells and
//! abilities are stored with a timestamp and lock in what they affect
//! (CR 611.2c). When nothing applies to an object, its characteristics are
//! its printed values and are read straight from the definition (the fast
//! path every game without continuous effects takes).

use mtg_core::{CardType, Colors, ObjectId, PlayerId, TypeLine};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use mtg_cards::ir::Ability;

use crate::card::{CardDef, DefId, Keywords};
use crate::effects::AbilityRef;
use crate::engine::Game;
use crate::state::ZoneId;

/// What a continuous effect does, by layer (CR 613.1).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum LayerEffect {
    /// Layer 7c: "+N/+M" (CR 613.4c).
    ModifyPt { power: i32, toughness: i32 },
}

/// How long an effect lasts (CR 611.2a–b).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Duration {
    /// "Until end of turn": ends in the cleanup step (CR 514.2).
    EndOfTurn,
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
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Characteristics {
    /// The definition supplying the copiable values (layer 1, CR 707.2).
    pub def: DefId,
    pub type_line: TypeLine,
    pub colors: Colors,
    /// Every ability the object has, as references into definitions.
    pub abilities: SmallVec<[AbilityRef; 6]>,
    pub keywords: Keywords,
    pub power: Option<i32>,
    pub toughness: Option<i32>,
    pub controller: PlayerId,
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
    Computed(Box<Characteristics>),
}

impl Chars<'_> {
    pub fn type_line(&self) -> &TypeLine {
        match self {
            Chars::Printed { def, .. } => &def.type_line,
            Chars::Computed(c) => &c.type_line,
        }
    }

    pub fn is(&self, t: CardType) -> bool {
        self.type_line().has_type(t)
    }

    pub fn colors(&self) -> Colors {
        match self {
            Chars::Printed { def, .. } => def.colors,
            Chars::Computed(c) => c.colors,
        }
    }

    pub fn keywords(&self) -> Keywords {
        match self {
            Chars::Printed { def, .. } => def.keywords,
            Chars::Computed(c) => c.keywords,
        }
    }

    pub fn controller(&self) -> PlayerId {
        match self {
            Chars::Printed { controller, .. } => *controller,
            Chars::Computed(c) => c.controller,
        }
    }
}

impl Game {
    /// A new timestamp (CR 613.7).
    pub(crate) fn next_timestamp(&mut self) -> u64 {
        self.timestamp += 1;
        self.timestamp
    }

    /// Whether any continuous effect applies to `id` (so its printed values
    /// aren't its characteristics).
    fn affected(&self, id: ObjectId) -> bool {
        self.effects.iter().any(|e| e.affected.contains(&id))
    }

    /// Object `id`'s characteristics (CR 613). Layers apply to permanents
    /// on the battlefield; elsewhere an object has its printed values.
    #[inline]
    pub(crate) fn chars(&self, id: ObjectId) -> Option<Chars<'_>> {
        let o = self.obj(id)?;
        let def = self.def(o.def);
        if self.effects.is_empty() || o.zone != ZoneId::Battlefield || !self.affected(id) {
            return Some(Chars::Printed {
                def_id: o.def,
                def,
                controller: o.controller,
            });
        }
        Some(Chars::Computed(Box::new(self.compute(id))))
    }

    /// Applies the layers to `id`'s printed values (CR 613.1).
    fn compute(&self, id: ObjectId) -> Characteristics {
        let o = self
            .obj(id)
            .copied()
            .unwrap_or_else(|| unreachable!("checked by chars()"));
        let def = self.def(o.def);
        let mut c = Characteristics {
            def: o.def,
            type_line: def.type_line.clone(),
            colors: def.colors,
            abilities: (0..def.abilities.len())
                .filter_map(|i| u8::try_from(i).ok())
                .map(|ability| AbilityRef {
                    def: o.def,
                    ability,
                })
                .collect(),
            keywords: def.keywords,
            power: def.power,
            toughness: def.toughness,
            controller: o.controller,
        };
        // Layer 7c (CR 613.4c): counters, then modifications in timestamp
        // order (they commute).
        let counters = self.counter_pt(id);
        let mut dp = counters;
        let mut dt = counters;
        let mut effects: SmallVec<[&ContinuousEffect; 4]> = self
            .effects
            .iter()
            .filter(|e| e.affected.contains(&id))
            .collect();
        effects.sort_by_key(|e| e.timestamp);
        for e in effects {
            match e.effect {
                LayerEffect::ModifyPt { power, toughness } => {
                    dp += power;
                    dt += toughness;
                }
            }
        }
        c.power = c.power.map(|p| p + dp);
        c.toughness = c.toughness.map(|t| t + dt);
        c
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
    }

    /// CR 514.2: "until end of turn" effects end in the cleanup step.
    pub(crate) fn end_turn_effects(&mut self) {
        self.effects.retain(|e| e.duration != Duration::EndOfTurn);
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
                .abilities
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
            Some(Chars::Computed(c)) => c
                .abilities
                .iter()
                .filter_map(|r| Some((*r, self.ability_at(*r)?)))
                .collect(),
        }
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
            Some(Chars::Computed(c)) => if power { c.power } else { c.toughness }.unwrap_or(0),
        }
    }

    #[inline]
    pub(crate) fn keywords(&self, id: ObjectId) -> Keywords {
        if self.effects.is_empty() {
            return self.obj_def(id).map(|d| d.keywords).unwrap_or_default();
        }
        self.chars(id).map(|c| c.keywords()).unwrap_or_default()
    }

    #[inline]
    pub(crate) fn is_creature(&self, id: ObjectId) -> bool {
        if self.effects.is_empty() {
            return self.obj_def(id).is_some_and(|d| d.is(CardType::Creature));
        }
        self.chars(id).is_some_and(|c| c.is(CardType::Creature))
    }
}
