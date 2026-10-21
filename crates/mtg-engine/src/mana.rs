//! Mana: cost requirements, available resources, exact payability, and
//! payment-plan enumeration (`docs/03 Architecture/Subsystems/Costs & Mana.md`, M2 plan D6).
//!
//! M2 mana sources produce one mana of one type per activation, and M2 costs
//! are generic, colored and `{C}` symbols only.

use std::collections::BTreeSet;

use mtg_core::{CardType, ManaCost, ManaSymbol, ObjectId, PlayerId};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::card::ManaType;
use crate::decision::PaymentPlan;
use crate::engine::Game;
use crate::events::Event;
use crate::state::ManaPool;

/// What remains to be paid of a mana cost (CR 202.1a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct CostReq {
    /// Colored symbols by [`ManaType`] index 0..5 (W U B R G).
    pub colored: [u32; 5],
    /// `{C}` symbols (CR 107.4c).
    pub colorless: u32,
    /// Generic mana (CR 107.4b).
    pub generic: u32,
}

impl CostReq {
    pub fn from_cost(cost: &ManaCost) -> CostReq {
        let mut r = CostReq::default();
        for s in cost.symbols() {
            match s {
                ManaSymbol::Generic(n) => r.generic += n,
                ManaSymbol::Colored(c) => r.colored[ManaType::of_color(*c).index()] += 1,
                ManaSymbol::Colorless => r.colorless += 1,
                // Rejected when the definition is built (card::CardDef::from_record).
                _ => {}
            }
        }
        r
    }

    pub fn is_paid(&self) -> bool {
        self.total() == 0
    }

    pub fn total(&self) -> u32 {
        self.colored.iter().sum::<u32>() + self.colorless + self.generic
    }

    /// Pays one unit with mana of type `t`, preferring a matching specific
    /// symbol. Returns false if `t` can't pay anything that remains.
    pub fn pay_one(&mut self, t: ManaType) -> bool {
        let i = t.index();
        if i < 5 && self.colored[i] > 0 {
            self.colored[i] -= 1;
        } else if t == ManaType::Colorless && self.colorless > 0 {
            self.colorless -= 1;
        } else if self.generic > 0 {
            self.generic -= 1;
        } else {
            return false;
        }
        true
    }

    /// Specific demand units: one entry per colored or `{C}` symbol.
    fn specific_units(&self) -> SmallVec<[ManaType; 8]> {
        let mut out = SmallVec::new();
        for t in ManaType::ALL {
            let n = if t == ManaType::Colorless {
                self.colorless
            } else {
                self.colored[t.index()]
            };
            for _ in 0..n {
                out.push(t);
            }
        }
        out
    }
}

/// An untapped mana source a player may activate: one permanent, the types
/// its abilities produce (one ability per type).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub object: ObjectId,
    /// `(ability index, mana type)`.
    pub abilities: SmallVec<[(u8, ManaType); 2]>,
    /// Interchangeability key: sources with equal keys leave identical outcomes.
    pub key: (u32, u32),
}

impl Source {
    fn mask(&self) -> u8 {
        self.abilities
            .iter()
            .fold(0, |m, (_, t)| m | (1 << t.index()))
    }
}

/// Bitmask-matching feasibility: can `pool` + `sources` pay `req`?
pub fn payable(req: &CostReq, pool: &ManaPool, sources: &[Source]) -> bool {
    let specific = req.specific_units();
    let need = specific.len();
    // Supply units: pool mana (capped, since more than the total demand never helps) and sources.
    let cap = req.total();
    let mut supply: SmallVec<[u8; 32]> = SmallVec::new();
    for t in ManaType::ALL {
        for _ in 0..pool.get(t).min(cap) {
            supply.push(1 << t.index());
        }
    }
    supply.extend(sources.iter().map(Source::mask));
    if supply.len() < req.total() as usize {
        return false;
    }
    // Kuhn's algorithm: match every specific symbol to a distinct supply unit.
    let mut owner: SmallVec<[usize; 32]> = SmallVec::from_elem(usize::MAX, supply.len());
    for d in 0..need {
        let mut seen: SmallVec<[bool; 32]> = SmallVec::from_elem(false, supply.len());
        if !augment(d, &specific, &supply, &mut owner, &mut seen) {
            return false;
        }
    }
    // Generic accepts anything; the remaining units just need to be enough.
    supply.len() - need >= req.generic as usize
}

fn augment(
    d: usize,
    demand: &[ManaType],
    supply: &[u8],
    owner: &mut [usize],
    seen: &mut [bool],
) -> bool {
    let bit = 1u8 << demand[d].index();
    for s in 0..supply.len() {
        if supply[s] & bit == 0 || seen[s] {
            continue;
        }
        seen[s] = true;
        if owner[s] == usize::MAX || augment(owner[s], demand, supply, owner, seen) {
            owner[s] = d;
            return true;
        }
    }
    false
}

/// Maximum plans offered per payment decision.
pub const MAX_PLANS: usize = 32;
const MAX_NODES: usize = 20_000;

/// A resource class: a pool type, or one ability of a group of interchangeable sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Pool(ManaType),
    Group {
        group: usize,
        ability: u8,
        mana: ManaType,
    },
}

impl Class {
    fn mana(self) -> ManaType {
        match self {
            Class::Pool(t) | Class::Group { mana: t, .. } => t,
        }
    }
}

/// Enumerates outcome-distinct payment plans, up to [`MAX_PLANS`].
pub fn enumerate_plans(req: &CostReq, pool: &ManaPool, sources: &[Source]) -> Vec<PaymentPlan> {
    // Group interchangeable sources.
    let mut groups: Vec<Vec<&Source>> = Vec::new();
    for s in sources {
        match groups
            .iter_mut()
            .find(|g| g[0].key == s.key && g[0].abilities == s.abilities)
        {
            Some(g) => g.push(s),
            None => groups.push(vec![s]),
        }
    }
    let mut classes: Vec<Class> = ManaType::ALL
        .iter()
        .filter(|t| pool.get(**t) > 0)
        .map(|t| Class::Pool(*t))
        .collect();
    for (gi, g) in groups.iter().enumerate() {
        for (ability, mana) in &g[0].abilities {
            classes.push(Class::Group {
                group: gi,
                ability: *ability,
                mana: *mana,
            });
        }
    }
    // Demand units: specific symbols (grouped by type) then generic.
    let mut demand: Vec<Option<ManaType>> = req.specific_units().into_iter().map(Some).collect();
    demand.extend(std::iter::repeat_n(None, req.generic as usize));

    let mut st = Search {
        classes: &classes,
        demand: &demand,
        pool_left: pool.0,
        group_left: groups.iter().map(Vec::len).collect(),
        usage: vec![0; classes.len()],
        found: BTreeSet::new(),
        nodes: 0,
    };
    st.dfs(0, 0);
    st.found
        .into_iter()
        .take(MAX_PLANS)
        .map(|usage| {
            let mut plan = PaymentPlan {
                activations: SmallVec::new(),
                spend: [0; 6],
            };
            let mut next_member = vec![0usize; groups.len()];
            for (ci, n) in usage.iter().enumerate() {
                let class = classes[ci];
                plan.spend[class.mana().index()] += n;
                if let Class::Group { group, ability, .. } = class {
                    for _ in 0..*n {
                        plan.activations
                            .push((groups[group][next_member[group]].object, ability));
                        next_member[group] += 1;
                    }
                }
            }
            plan
        })
        .collect()
}

struct Search<'a> {
    classes: &'a [Class],
    demand: &'a [Option<ManaType>],
    pool_left: [u32; 6],
    group_left: Vec<usize>,
    usage: Vec<u32>,
    found: BTreeSet<Vec<u32>>,
    nodes: usize,
}

impl Search<'_> {
    /// Assigns demand unit `d` to a class with index ≥ `min_class` when the
    /// previous unit had the same demand kind (avoids permutations).
    fn dfs(&mut self, d: usize, min_class: usize) {
        self.nodes += 1;
        if self.found.len() >= MAX_PLANS || self.nodes > MAX_NODES {
            return;
        }
        if d == self.demand.len() {
            self.found.insert(self.usage.clone());
            return;
        }
        let kind = self.demand[d];
        let start = if d > 0 && self.demand[d - 1] == kind {
            min_class
        } else {
            0
        };
        for ci in start..self.classes.len() {
            let class = self.classes[ci];
            if let Some(t) = kind
                && class.mana() != t
            {
                continue;
            }
            let ok = match class {
                Class::Pool(t) => self.pool_left[t.index()] > 0,
                Class::Group { group, .. } => self.group_left[group] > 0,
            };
            if !ok {
                continue;
            }
            match class {
                Class::Pool(t) => self.pool_left[t.index()] -= 1,
                Class::Group { group, .. } => self.group_left[group] -= 1,
            }
            self.usage[ci] += 1;
            self.dfs(d + 1, ci);
            self.usage[ci] -= 1;
            match class {
                Class::Pool(t) => self.pool_left[t.index()] += 1,
                Class::Group { group, .. } => self.group_left[group] += 1,
            }
        }
    }
}

impl Game {
    /// Untapped mana sources `p` can activate (CR 605.3a), excluding `except`.
    pub(crate) fn mana_sources(&self, p: PlayerId, except: Option<ObjectId>) -> Vec<Source> {
        let mut out = Vec::new();
        for &id in &self.battlefield {
            if Some(id) == except {
                continue;
            }
            let Some(o) = self.obj(id) else { continue };
            if o.controller != p || o.tapped {
                continue;
            }
            let def = self.def(o.def);
            if def.mana_abilities.is_empty()
                || (def.is(CardType::Creature) && self.is_summoning_sick(id))
            {
                continue;
            }
            let in_combat = self
                .combat
                .as_ref()
                .is_some_and(|c| c.attacker(id).is_some() || c.is_blocking(id));
            out.push(Source {
                object: id,
                // M3 B1: every supported mana ability makes one mana of one type.
                abilities: def
                    .mana_abilities
                    .iter()
                    .enumerate()
                    .filter_map(|(i, m)| Some((u8::try_from(i).ok()?, m.single_type()?)))
                    .collect(),
                key: (o.def.0, o.damage | (u32::from(in_combat) << 31)),
            });
        }
        out
    }

    /// Activates a mana ability: tap the source, add the mana (CR 605.3b, no stack).
    pub(crate) fn activate_mana(&mut self, source: ObjectId, ability: u8) {
        let Some(o) = self.obj(source).copied() else {
            return;
        };
        let Some(mana) = self
            .def(o.def)
            .mana_abilities
            .get(ability as usize)
            .and_then(crate::card::ManaAbility::single_type)
        else {
            return;
        };
        self.tap(source);
        self.player_mut(o.controller).pool.add(mana, 1);
        self.log(Event::ManaAdded {
            player: o.controller,
            mana,
            source,
        });
    }

    /// Executes a payment plan: activate its sources, then spend from the pool.
    pub(crate) fn execute_plan(&mut self, p: PlayerId, plan: &PaymentPlan) {
        for (source, ability) in &plan.activations {
            self.activate_mana(*source, *ability);
        }
        let pool = &mut self.player_mut(p).pool;
        for t in ManaType::ALL {
            let n = plan.spend[t.index()];
            debug_assert!(pool.get(t) >= n, "plan spends more {t} than the pool holds");
            pool.0[t.index()] = pool.get(t).saturating_sub(n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(i: u32, def: u32, types: &[ManaType]) -> Source {
        Source {
            object: ObjectId {
                index: i,
                generation: 0,
            },
            abilities: types
                .iter()
                .enumerate()
                .map(|(k, t)| (u8::try_from(k).unwrap_or(0), *t))
                .collect(),
            key: (def, 0),
        }
    }

    fn req(s: &str) -> CostReq {
        CostReq::from_cost(&s.parse().unwrap_or_default())
    }

    #[test]
    fn feasibility() {
        let forest = |i| src(i, 1, &[ManaType::Green]);
        let island = |i| src(i, 2, &[ManaType::Blue]);
        let pool = ManaPool::default();
        assert!(payable(&req("{1}{G}"), &pool, &[forest(0), island(1)]));
        assert!(!payable(&req("{G}{G}"), &pool, &[forest(0), island(1)]));
        assert!(!payable(&req("{2}{G}"), &pool, &[forest(0), island(1)]));
        // A dual source covers either color but only once.
        let dual = src(3, 3, &[ManaType::Green, ManaType::Blue]);
        assert!(payable(&req("{G}{U}"), &pool, &[forest(0), dual.clone()]));
        assert!(!payable(&req("{G}{U}{U}"), &pool, &[forest(0), dual]));
        let mut floating = ManaPool::default();
        floating.add(ManaType::Colorless, 2);
        assert!(payable(&req("{2}"), &floating, &[]));
        assert!(!payable(&req("{C}{G}"), &floating, &[island(1)]));
    }

    #[test]
    fn plans_are_outcome_distinct() {
        let pool = ManaPool::default();
        // Three identical Forests and an Island: {1}{G} can be paid as G+G or G+U.
        let sources = vec![
            src(0, 1, &[ManaType::Green]),
            src(1, 1, &[ManaType::Green]),
            src(2, 1, &[ManaType::Green]),
            src(3, 2, &[ManaType::Blue]),
        ];
        let plans = enumerate_plans(&req("{1}{G}"), &pool, &sources);
        assert_eq!(plans.len(), 2, "{plans:?}");
        for plan in &plans {
            assert_eq!(plan.activations.len(), 2);
            assert_eq!(plan.spend.iter().sum::<u32>(), 2);
        }
    }

    #[test]
    fn plans_use_pool_mana() {
        let mut pool = ManaPool::default();
        pool.add(ManaType::Red, 1);
        let plans = enumerate_plans(&req("{R}"), &pool, &[src(0, 4, &[ManaType::Red])]);
        // Spend the floating R, or tap the Mountain.
        assert_eq!(plans.len(), 2);
        assert!(
            plans
                .iter()
                .any(|p| p.activations.is_empty() && p.spend[ManaType::Red.index()] == 1)
        );
    }
}
