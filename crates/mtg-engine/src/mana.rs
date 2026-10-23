//! Mana: cost requirements, available resources, exact payability, and
//! payment-plan enumeration (`docs/03 Architecture/Subsystems/Costs & Mana.md`,
//! M2 plan D6, M3 plan B-D8).
//!
//! Sources are untapped permanents whose mana abilities cost only {T} and
//! need no choices besides their mana. An activation makes one of the
//! ability's outcome bags (Sol Ring: {C}{C}; Birds of Paradise: one mana of
//! any color). Other mana abilities (sacrifice, life, mana costs) are
//! activated while holding priority, floating their mana first; that has the
//! same outcome as activating them during payment (CR 605.3a).

use std::collections::BTreeSet;

use mtg_core::{ManaCost, ManaSymbol, ObjectId, PlayerId};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::card::{ManaBag, ManaType};
use crate::decision::PaymentPlan;
use crate::effects::{AbilityRef, EffectCtx, EffectTask};
use crate::engine::{Game, Task};
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
                // Hybrid, Phyrexian and X are resolved by the cast procedure (CR 601.2b).
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

/// Outcome bags of one mana ability.
pub type Bags = SmallVec<[ManaBag; 1]>;

/// An untapped mana source a player may activate during payment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub object: ObjectId,
    /// `(index into the definition's mana abilities, outcome bags)`.
    pub abilities: SmallVec<[(u8, Bags); 2]>,
    /// Interchangeability key: sources with equal keys leave identical outcomes.
    pub key: (u32, u32),
}

fn bag_total(b: &ManaBag) -> u32 {
    b.iter().map(|n| u32::from(*n)).sum()
}

impl Source {
    /// If every outcome is one mana: the mask of types it can be.
    fn single_mask(&self) -> Option<u8> {
        let mut mask = 0u8;
        for (_, bags) in &self.abilities {
            for b in bags {
                if bag_total(b) != 1 {
                    return None;
                }
                for t in ManaType::ALL {
                    if b[t.index()] > 0 {
                        mask |= t.bit();
                    }
                }
            }
        }
        Some(mask)
    }

    /// Every distinct outcome of activating this source once.
    fn outcomes(&self) -> Vec<ManaBag> {
        let mut v: Vec<ManaBag> = self
            .abilities
            .iter()
            .flat_map(|(_, b)| b.iter().copied())
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

/// Most outcome combinations of multi-mana sources tried by [`payable`].
const MAX_COMBINATIONS: usize = 4096;

/// Exact feasibility: can `pool` plus `sources` pay `req`?
pub fn payable(req: &CostReq, pool: &ManaPool, sources: &[Source]) -> bool {
    let cap = req.total();
    // Single-mana supply: pool mana (capped: more than the demand never helps) and sources.
    let mut base: SmallVec<[u8; 32]> = SmallVec::new();
    for t in ManaType::ALL {
        for _ in 0..pool.get(t).min(cap) {
            base.push(t.bit());
        }
    }
    let mut multi: Vec<Vec<ManaBag>> = Vec::new();
    for s in sources {
        match s.single_mask() {
            Some(mask) => base.push(mask),
            None => multi.push(s.outcomes()),
        }
    }
    if multi.is_empty() {
        return matches(req, &base);
    }
    // Try each combination of outcomes of the multi-mana sources.
    let mut idx = vec![0usize; multi.len()];
    for _ in 0..MAX_COMBINATIONS {
        let mut supply = base.clone();
        for (o, i) in multi.iter().zip(&idx) {
            let bag = o[*i];
            for t in ManaType::ALL {
                for _ in 0..bag[t.index()] {
                    supply.push(t.bit());
                }
            }
        }
        if matches(req, &supply) {
            return true;
        }
        let mut k = 0;
        loop {
            if k == idx.len() {
                return false;
            }
            idx[k] += 1;
            if idx[k] < multi[k].len() {
                break;
            }
            idx[k] = 0;
            k += 1;
        }
    }
    false
}

/// Kuhn's algorithm: every specific symbol gets a distinct supply unit, and
/// enough units remain for the generic part.
fn matches(req: &CostReq, supply: &[u8]) -> bool {
    if supply.len() < req.total() as usize {
        return false;
    }
    let specific = req.specific_units();
    let mut owner: SmallVec<[usize; 32]> = SmallVec::from_elem(usize::MAX, supply.len());
    for d in 0..specific.len() {
        let mut seen: SmallVec<[bool; 32]> = SmallVec::from_elem(false, supply.len());
        if !augment(d, &specific, supply, &mut owner, &mut seen) {
            return false;
        }
    }
    supply.len() - specific.len() >= req.generic as usize
}

fn augment(
    d: usize,
    demand: &[ManaType],
    supply: &[u8],
    owner: &mut [usize],
    seen: &mut [bool],
) -> bool {
    let bit = demand[d].bit();
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

/// One way to activate a group of interchangeable sources: an ability and its outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActClass {
    group: usize,
    ability: u8,
    bag: ManaBag,
}

/// Enumerates outcome-distinct payment plans, up to [`MAX_PLANS`].
///
/// Class indices 0..6 are pool mana by type; activations follow. Consecutive
/// demand units of the same kind take non-decreasing classes, which skips
/// permutations; mana floated by an earlier activation may always be spent.
/// With one-mana sources this is exactly the M2 search and order.
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
    let mut classes: Vec<ActClass> = Vec::new();
    for (gi, g) in groups.iter().enumerate() {
        for (ability, bags) in &g[0].abilities {
            for bag in bags {
                classes.push(ActClass {
                    group: gi,
                    ability: *ability,
                    bag: *bag,
                });
            }
        }
    }
    // Demand units: specific symbols (grouped by type) then generic.
    let mut demand: Vec<Option<ManaType>> = req.specific_units().into_iter().map(Some).collect();
    demand.extend(std::iter::repeat_n(None, req.generic as usize));

    let mut st = Search {
        classes: &classes,
        demand: &demand,
        initial: pool.0,
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
        .map(|(_, usage, final_pool)| {
            let mut plan = PaymentPlan {
                activations: SmallVec::new(),
                spend: [0; 6],
            };
            let mut produced = [0u32; 6];
            let mut next_member = vec![0usize; groups.len()];
            for (ci, n) in usage.iter().enumerate() {
                let c = classes[ci];
                for _ in 0..*n {
                    plan.activations.push((
                        groups[c.group][next_member[c.group]].object,
                        c.ability,
                        c.bag,
                    ));
                    next_member[c.group] += 1;
                    for t in ManaType::ALL {
                        produced[t.index()] += u32::from(c.bag[t.index()]);
                    }
                }
            }
            for t in ManaType::ALL {
                let i = t.index();
                plan.spend[i] = (pool.0[i] + produced[i]).saturating_sub(final_pool[i]);
            }
            plan
        })
        .collect()
}

/// (sort key, activation usage per class, final pool)
type Found = (Vec<i64>, Vec<u32>, [u32; 6]);

struct Search<'a> {
    classes: &'a [ActClass],
    demand: &'a [Option<ManaType>],
    initial: [u32; 6],
    pool_left: [u32; 6],
    group_left: Vec<usize>,
    usage: Vec<u32>,
    found: BTreeSet<Found>,
    nodes: usize,
}

impl Search<'_> {
    fn record(&mut self) {
        // M2's order: pool usage for the types the pool started with, then activations.
        let mut key: Vec<i64> = ManaType::ALL
            .iter()
            .filter(|t| self.initial[t.index()] > 0)
            .map(|t| i64::from(self.initial[t.index()]) - i64::from(self.pool_left[t.index()]))
            .collect();
        key.extend(self.usage.iter().map(|u| i64::from(*u)));
        self.found.insert((key, self.usage.clone(), self.pool_left));
    }

    fn dfs(&mut self, d: usize, min_class: usize) {
        self.nodes += 1;
        if self.found.len() >= MAX_PLANS || self.nodes > MAX_NODES {
            return;
        }
        if d == self.demand.len() {
            self.record();
            return;
        }
        let kind = self.demand[d];
        let start = if d > 0 && self.demand[d - 1] == kind {
            min_class
        } else {
            0
        };
        // Pool mana (classes 0..6).
        for t in ManaType::ALL {
            let i = t.index();
            if kind.is_some_and(|k| k != t) || self.pool_left[i] == 0 {
                continue;
            }
            // Floated mana from an activation may be spent regardless of order.
            let floated = self.pool_left[i] > self.initial[i];
            if i < start && !floated {
                continue;
            }
            self.pool_left[i] -= 1;
            self.dfs(d + 1, i.max(start));
            self.pool_left[i] += 1;
        }
        // Activations (classes 6..).
        for ci in start.saturating_sub(6)..self.classes.len() {
            let c = self.classes[ci];
            if self.group_left[c.group] == 0 {
                continue;
            }
            self.group_left[c.group] -= 1;
            self.usage[ci] += 1;
            for t in ManaType::ALL {
                self.pool_left[t.index()] += u32::from(c.bag[t.index()]);
            }
            for t in ManaType::ALL {
                let i = t.index();
                if c.bag[i] == 0 || kind.is_some_and(|k| k != t) {
                    continue;
                }
                self.pool_left[i] -= 1;
                self.dfs(d + 1, 6 + ci);
                self.pool_left[i] += 1;
            }
            for t in ManaType::ALL {
                self.pool_left[t.index()] -= u32::from(c.bag[t.index()]);
            }
            self.usage[ci] -= 1;
            self.group_left[c.group] += 1;
        }
    }
}

impl Game {
    /// Untapped mana sources `p` can activate during payment (CR 605.3a),
    /// excluding `except` (a permanent whose {T} is part of the cost being paid).
    pub(crate) fn mana_sources(&self, p: PlayerId, except: Option<ObjectId>) -> Vec<Source> {
        let mut out = Vec::new();
        for &id in &self.battlefield {
            if Some(id) == except {
                continue;
            }
            let Some(o) = self.obj(id) else { continue };
            if o.controller != p || o.tapped || !self.activation_allowed(p, id) {
                continue;
            }
            let mana = self.mana_abilities_of(id);
            // CR 302.6: a creature's {T} abilities need it under control since the turn began.
            if mana.is_empty() || (self.is_creature(id) && self.is_summoning_sick(id)) {
                continue;
            }
            let abilities: SmallVec<[(u8, Bags); 2]> = mana
                .iter()
                .enumerate()
                .filter(|(_, (_, m))| m.tap_only)
                .filter(|(_, (r, _))| self.activation_condition_holds(*r, id, p))
                .filter_map(|(i, (r, m))| {
                    Some((u8::try_from(i).ok()?, self.mana_ability_bags(*r, m, p)))
                })
                .collect();
            if abilities.is_empty() {
                continue;
            }
            let in_combat = self
                .combat
                .as_ref()
                .is_some_and(|c| c.attacker(id).is_some() || c.is_blocking(id));
            out.push(Source {
                object: id,
                abilities,
                key: (o.def.0, o.damage | (u32::from(in_combat) << 31)),
            });
        }
        out
    }

    /// CR 602.5b: whether ability `r` of `source`'s "Activate only if" holds.
    pub(crate) fn activation_condition_holds(
        &self,
        r: AbilityRef,
        source: ObjectId,
        p: PlayerId,
    ) -> bool {
        match self.ability_at(r) {
            Some(mtg_cards::ir::Ability::Activated(x)) => x
                .condition
                .as_ref()
                .is_none_or(|c| self.condition(&EffectCtx::new(r, source, p), c)),
            _ => true,
        }
    }

    /// The outcomes of one activation of mana ability `m` (ability `r`)
    /// controlled by `p`: precomputed, or computed now for dynamic mana.
    pub(crate) fn mana_ability_bags(
        &self,
        r: AbilityRef,
        m: &crate::card::ManaAbility,
        p: PlayerId,
    ) -> SmallVec<[crate::card::ManaBag; 1]> {
        if !m.dynamic {
            return m.bags.clone();
        }
        let mut out = SmallVec::new();
        if let Some(mtg_cards::ir::Ability::Activated(x)) =
            self.def(r.def).abilities.get(usize::from(r.ability))
        {
            x.effect.walk(&mut |e| {
                if let mtg_cards::ir::Effect::AddMana { mana, .. } = e {
                    out.extend(self.dynamic_mana_bags(mana, p));
                }
            });
        }
        out
    }

    /// Outcomes of a mana spec that depends on the game, for player `p`.
    pub(crate) fn dynamic_mana_bags(
        &self,
        spec: &mtg_cards::ir::ManaSpec,
        p: PlayerId,
    ) -> Vec<crate::card::ManaBag> {
        match spec {
            // CR 903.4f: no commander, no mana; a colorless identity, none either.
            mtg_cards::ir::ManaSpec::CommanderIdentity => self
                .commander_identity(p)
                .map(|id| {
                    id.iter()
                        .map(|c| {
                            let mut b = [0; 6];
                            b[ManaType::of_color(c).index()] = 1;
                            b
                        })
                        .collect()
                })
                .unwrap_or_default(),
            other => crate::effects::mana_bags(other),
        }
    }

    /// The mana abilities object `id` has now (CR 605.1a), after the layers:
    /// `(the ability, its precomputed output)`. Mana-ability indices in
    /// actions and payment plans index this list.
    pub(crate) fn mana_abilities_of(
        &self,
        id: ObjectId,
    ) -> SmallVec<[(AbilityRef, &crate::card::ManaAbility); 2]> {
        match self.chars(id) {
            None => SmallVec::new(),
            Some(crate::layers::Chars::Printed { def_id, def, .. }) => def
                .mana_abilities
                .iter()
                .map(|m| {
                    (
                        AbilityRef {
                            def: def_id,
                            ability: m.ability,
                        },
                        m,
                    )
                })
                .collect(),
            Some(crate::layers::Chars::Computed(..)) => self
                .abilities_of(id)
                .into_iter()
                .filter_map(|(r, _)| {
                    let m = self
                        .def(r.def)
                        .mana_abilities
                        .iter()
                        .find(|m| m.ability == r.ability)?;
                    Some((r, m))
                })
                .collect(),
        }
    }

    /// Mana ability `i` of object `id` (an index into
    /// [`Game::mana_abilities_of`]), without building the list when the
    /// object has its printed abilities.
    pub(crate) fn mana_ability_at(
        &self,
        id: ObjectId,
        i: u8,
    ) -> Option<(AbilityRef, &crate::card::ManaAbility)> {
        if !self.layers_live() {
            let o = self.obj(id)?;
            let m = self.def(o.def).mana_abilities.get(usize::from(i))?;
            return Some((
                AbilityRef {
                    def: o.def,
                    ability: m.ability,
                },
                m,
            ));
        }
        self.mana_abilities_of(id).get(usize::from(i)).copied()
    }

    /// How many mana abilities object `id` has now.
    pub(crate) fn mana_ability_count(&self, id: ObjectId) -> usize {
        if !self.layers_live() {
            return self.obj_def(id).map_or(0, |d| d.mana_abilities.len());
        }
        self.mana_abilities_of(id).len()
    }

    /// Whether `p` may activate mana ability `ability` (an index into
    /// [`Game::mana_abilities_of`]) of `source` right now (CR 605.3a, 302.6).
    pub(crate) fn can_activate_mana(&self, p: PlayerId, source: ObjectId, ability: u8) -> bool {
        let Some(o) = self.obj(source) else {
            return false;
        };
        if o.controller != p
            || o.zone != crate::state::ZoneId::Battlefield
            || !self.activation_allowed(p, source)
        {
            return false;
        }
        let Some((r, _)) = self.mana_ability_at(source, ability) else {
            return false;
        };
        let Some(mtg_cards::ir::Ability::Activated(x)) = self.ability_at(r) else {
            return false;
        };
        let taps = x.cost.iter().any(|c| {
            matches!(
                c,
                mtg_cards::ir::CostPart::Tap | mtg_cards::ir::CostPart::Untap
            )
        });
        if taps && self.is_creature(source) && self.is_summoning_sick(source) {
            return false;
        }
        let ctx = EffectCtx::new(r, source, p);
        // CR 602.5b: "Activate only if ..." (Mox Opal's metalcraft).
        if x.condition
            .as_ref()
            .is_some_and(|c| !self.condition(&ctx, c))
        {
            return false;
        }
        self.can_pay(&ctx, p, &x.cost)
    }

    /// Activates a mana ability (CR 605.3b: it doesn't use the stack): pays its
    /// costs, then runs its effect. `forced` fixes which mana it makes
    /// (payment plans and manual steps); otherwise the player chooses.
    /// Pushes a task, so callers push their continuation first.
    pub(crate) fn activate_mana(&mut self, source: ObjectId, ability: u8, forced: Option<ManaBag>) {
        let Some(o) = self.obj(source).copied() else {
            return;
        };
        let Some((r, _)) = self.mana_ability_at(source, ability) else {
            return;
        };
        let pool = std::sync::Arc::clone(&self.pool);
        let Some(mtg_cards::ir::Ability::Activated(x)) =
            pool.get(r.def).abilities.get(usize::from(r.ability))
        else {
            return;
        };
        let mut ctx = EffectCtx::new(r, source, o.controller);
        ctx.forced_mana = forced;
        ctx.tapped_for_mana = x
            .cost
            .iter()
            .any(|c| matches!(c, mtg_cards::ir::CostPart::Tap));
        let mut task = EffectTask::new(ctx);
        // Costs first (they go on top of the effect).
        let cost = x.cost.clone();
        task.push_costs(o.controller, &cost);
        self.tasks.push(Task::Effect(Box::new(task)));
    }

    /// Executes a payment plan: activate its sources, then spend from the pool.
    /// Pushes tasks, so callers push their continuation first.
    pub(crate) fn execute_plan(&mut self, p: PlayerId, plan: &PaymentPlan) {
        self.tasks.push(Task::SpendMana {
            player: p,
            spend: plan.spend,
        });
        for (source, ability, bag) in plan.activations.iter().rev() {
            self.activate_mana(*source, *ability, Some(*bag));
        }
    }

    /// Spends mana from a player's pool (the last step of a payment plan).
    pub(crate) fn spend_mana(&mut self, p: PlayerId, spend: [u32; 6]) {
        let pool = &mut self.player_mut(p).pool;
        for t in ManaType::ALL {
            let n = spend[t.index()];
            debug_assert!(pool.get(t) >= n, "plan spends more {t} than the pool holds");
            pool.0[t.index()] = pool.get(t).saturating_sub(n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bag(t: ManaType, n: u8) -> ManaBag {
        let mut b = [0; 6];
        b[t.index()] = n;
        b
    }

    fn src(i: u32, def: u32, outcomes: &[ManaBag]) -> Source {
        Source {
            object: ObjectId {
                index: i,
                generation: 0,
            },
            abilities: SmallVec::from_elem((0, outcomes.iter().copied().collect()), 1),
            key: (def, 0),
        }
    }

    fn land(i: u32, def: u32, t: ManaType) -> Source {
        src(i, def, &[bag(t, 1)])
    }

    fn req(s: &str) -> CostReq {
        CostReq::from_cost(&s.parse().unwrap_or_default())
    }

    #[test]
    fn feasibility() {
        let forest = |i| land(i, 1, ManaType::Green);
        let island = |i| land(i, 2, ManaType::Blue);
        let pool = ManaPool::default();
        assert!(payable(&req("{1}{G}"), &pool, &[forest(0), island(1)]));
        assert!(!payable(&req("{G}{G}"), &pool, &[forest(0), island(1)]));
        assert!(!payable(&req("{2}{G}"), &pool, &[forest(0), island(1)]));
        // A dual source covers either color but only once.
        let dual = src(3, 3, &[bag(ManaType::Green, 1), bag(ManaType::Blue, 1)]);
        assert!(payable(&req("{G}{U}"), &pool, &[forest(0), dual.clone()]));
        assert!(!payable(&req("{G}{U}{U}"), &pool, &[forest(0), dual]));
        let mut floating = ManaPool::default();
        floating.add(ManaType::Colorless, 2);
        assert!(payable(&req("{2}"), &floating, &[]));
        assert!(!payable(&req("{C}{G}"), &floating, &[island(1)]));
    }

    #[test]
    fn multi_mana_sources() {
        let pool = ManaPool::default();
        let sol = src(0, 9, &[bag(ManaType::Colorless, 2)]);
        assert!(payable(&req("{2}"), &pool, std::slice::from_ref(&sol)));
        assert!(payable(&req("{C}{C}"), &pool, std::slice::from_ref(&sol)));
        assert!(!payable(&req("{1}{G}"), &pool, std::slice::from_ref(&sol)));
        assert!(payable(
            &req("{2}{G}"),
            &pool,
            &[sol.clone(), land(1, 1, ManaType::Green)]
        ));
        // One activation of "three mana of any one color".
        let lotus: Vec<ManaBag> = ManaType::ALL[..5].iter().map(|t| bag(*t, 3)).collect();
        let lotus = src(2, 8, &lotus);
        assert!(payable(
            &req("{U}{U}{U}"),
            &pool,
            std::slice::from_ref(&lotus)
        ));
        assert!(!payable(
            &req("{U}{G}"),
            &pool,
            std::slice::from_ref(&lotus)
        ));
        // Plans: Sol Ring alone pays {2} (one plan); {1} leaves {C} floating.
        let plans = enumerate_plans(&req("{2}"), &pool, std::slice::from_ref(&sol));
        assert_eq!(plans.len(), 1, "{plans:?}");
        assert_eq!(plans[0].spend, bag(ManaType::Colorless, 2).map(u32::from));
        let plans = enumerate_plans(&req("{1}"), &pool, &[sol]);
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].spend.iter().sum::<u32>(), 1);
    }

    #[test]
    fn plans_are_outcome_distinct() {
        let pool = ManaPool::default();
        // Three identical Forests and an Island: {1}{G} can be paid as G+G or G+U.
        let sources = vec![
            land(0, 1, ManaType::Green),
            land(1, 1, ManaType::Green),
            land(2, 1, ManaType::Green),
            land(3, 2, ManaType::Blue),
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
        let plans = enumerate_plans(&req("{R}"), &pool, &[land(0, 4, ManaType::Red)]);
        // Spend the floating R, or tap the Mountain.
        assert_eq!(plans.len(), 2);
        assert!(
            plans
                .iter()
                .any(|p| p.activations.is_empty() && p.spend[ManaType::Red.index()] == 1)
        );
    }
}
