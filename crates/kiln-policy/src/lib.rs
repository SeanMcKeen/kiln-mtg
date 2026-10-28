//! `kiln-policy`: decision-makers (`docs/14 Players & Policies/Policy Interface.md`).
//!
//! Every decision-maker implements [`Policy`] and sees only a
//! [`PlayerView`], never the game state (FR-P6). Policies are deterministic
//! given their seed, which comes from a stream separate from the game's
//! (ADR-0005), so changing one seat's policy never changes another's shuffles.

use std::collections::VecDeque;

mod greedy;

pub use greedy::GreedyPolicy;
use kiln_engine::rng::GameRng;
use kiln_engine::{DecisionKind, PendingDecision, PlayerView};
use kiln_types::PlayerId;

/// A decision-maker for one seat.
pub trait Policy: Send {
    /// A short name for reports.
    fn name(&self) -> String;

    /// Called once per game before the first decision.
    fn begin_game(&mut self, _seat: PlayerId, _seed: u64) {}

    /// Returns the index of the chosen option. It must be `< decision.option_count()`.
    fn decide(&mut self, view: &PlayerView<'_>, decision: &PendingDecision) -> u32;

    /// Like [`Policy::decide`], but may decline (a playbook with no rule for
    /// this decision); a composed policy then asks the next one.
    fn try_decide(&mut self, view: &PlayerView<'_>, decision: &PendingDecision) -> Option<u32> {
        Some(self.decide(view, decision))
    }

    /// Counters for reports since `begin_game`: which component answered,
    /// playbook tags, errors.
    fn stats(&self) -> std::collections::BTreeMap<String, u64> {
        std::collections::BTreeMap::new()
    }

    /// Runtime error messages since the last call (playbooks), for reports.
    fn take_errors(&mut self) -> Vec<String> {
        Vec::new()
    }

    /// Why the last decision was made, when the policy can say (an LLM's
    /// reason, the options search weighed). Annotates replays (M10 plan L5).
    fn rationale(&self) -> Option<String> {
        None
    }
}

/// Asks each policy in turn; the first that answers decides
/// (`docs/14 Players & Policies/Policy Composition.md`, `Fallback`). The last
/// one must always answer.
pub struct Compose {
    pub chain: Vec<Box<dyn Policy>>,
    answered: Vec<u64>,
    /// Which policy answered the last decision.
    last: Option<usize>,
}

impl std::fmt::Debug for Compose {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Compose")
            .field("chain", &self.name())
            .finish()
    }
}

impl Compose {
    pub fn new(chain: Vec<Box<dyn Policy>>) -> Self {
        let n = chain.len();
        Self {
            chain,
            answered: vec![0; n],
            last: None,
        }
    }
}

impl Policy for Compose {
    fn name(&self) -> String {
        self.chain
            .iter()
            .map(|p| p.name())
            .collect::<Vec<_>>()
            .join(" → ")
    }

    fn begin_game(&mut self, seat: PlayerId, seed: u64) {
        for (i, p) in self.chain.iter_mut().enumerate() {
            p.begin_game(seat, seed.wrapping_add(i as u64));
        }
        self.answered.iter_mut().for_each(|a| *a = 0);
    }

    fn try_decide(&mut self, view: &PlayerView<'_>, decision: &PendingDecision) -> Option<u32> {
        for (i, p) in self.chain.iter_mut().enumerate() {
            if let Some(c) = p.try_decide(view, decision) {
                self.answered[i] += 1;
                self.last = Some(i);
                return Some(c);
            }
        }
        None
    }

    fn rationale(&self) -> Option<String> {
        self.last.and_then(|i| self.chain[i].rationale())
    }

    fn decide(&mut self, view: &PlayerView<'_>, decision: &PendingDecision) -> u32 {
        if let Some(c) = self.try_decide(view, decision) {
            return c;
        }
        let last = self.chain.len().saturating_sub(1);
        self.answered[last] += 1;
        self.last = Some(last);
        self.chain[last].decide(view, decision)
    }

    fn take_errors(&mut self) -> Vec<String> {
        self.chain
            .iter_mut()
            .flat_map(|p| p.take_errors())
            .collect()
    }

    fn stats(&self) -> std::collections::BTreeMap<String, u64> {
        let mut out = std::collections::BTreeMap::new();
        for (p, n) in self.chain.iter().zip(&self.answered) {
            *out.entry(format!("answered:{}", p.name())).or_default() += n;
            for (k, v) in p.stats() {
                *out.entry(k).or_default() += v;
            }
        }
        out
    }
}

/// Uniform over legal options, with a configurable bias toward passing
/// priority (otherwise random play acts absurdly often).
#[derive(Debug, Clone)]
pub struct RandomPolicy {
    rng: GameRng,
    /// Chance, in parts per thousand, of passing when passing is legal.
    pub pass_permille: u64,
    /// Chance, in parts per thousand, of keeping at a mulligan decision.
    pub keep_permille: u64,
}

impl RandomPolicy {
    pub fn new(pass_permille: u64) -> Self {
        Self {
            rng: GameRng::new(0),
            pass_permille,
            keep_permille: 850,
        }
    }
}

impl Default for RandomPolicy {
    fn default() -> Self {
        Self::new(500)
    }
}

impl Policy for RandomPolicy {
    fn name(&self) -> String {
        format!("random(pass={}‰)", self.pass_permille)
    }

    fn begin_game(&mut self, _seat: PlayerId, seed: u64) {
        self.rng = GameRng::new(seed);
    }

    fn decide(&mut self, _view: &PlayerView<'_>, d: &PendingDecision) -> u32 {
        let n = d.option_count();
        match d.kind {
            DecisionKind::Priority { .. } if self.rng.below(1000) < self.pass_permille => return 0,
            DecisionKind::Mulligan { .. } => {
                return u32::from(self.rng.below(1000) >= self.keep_permille);
            }
            _ => {}
        }
        u32::try_from(self.rng.index(n)).unwrap_or(0)
    }
}

/// Always takes option 0: pass, keep, no attack, no block; otherwise the
/// first legal option (`docs/14 Players & Policies/A - Baseline Policies.md`).
#[derive(Debug, Clone, Copy, Default)]
pub struct PassivePolicy;

impl Policy for PassivePolicy {
    fn name(&self) -> String {
        "passive".into()
    }

    fn decide(&mut self, _view: &PlayerView<'_>, _d: &PendingDecision) -> u32 {
        0
    }
}

/// Follows a fixed list of option indices, then falls back to option 0.
#[derive(Debug, Clone, Default)]
pub struct ScriptedPolicy {
    script: VecDeque<u32>,
    /// Decisions answered after the script ran out.
    pub overrun: u32,
}

impl ScriptedPolicy {
    pub fn new(script: impl IntoIterator<Item = u32>) -> Self {
        Self {
            script: script.into_iter().collect(),
            overrun: 0,
        }
    }
}

impl Policy for ScriptedPolicy {
    fn name(&self) -> String {
        "scripted".into()
    }

    fn decide(&mut self, _view: &PlayerView<'_>, d: &PendingDecision) -> u32 {
        match self.script.pop_front() {
            Some(i) if (i as usize) < d.option_count() => i,
            Some(_) | None => {
                self.overrun += 1;
                0
            }
        }
    }
}

/// Whether a decision is worth a real choice (M10 plan L2): mulligans,
/// priority with at least two actions besides mana abilities, targets,
/// attacks, blocks, discards, modes, names, players, card choices that
/// search, take, discard, sacrifice or exile, and "may" choices. Search (M9)
/// searches these, live play and the LLM adapter ask about them, and other
/// decisions (payments, ordering, bottoming) go to a fallback.
pub fn is_key_decision(d: &PendingDecision) -> bool {
    use kiln_engine::Action;
    use kiln_engine::decision::{CardPurpose, OptionalKind};
    match &d.kind {
        DecisionKind::Priority { actions } => {
            actions
                .iter()
                .filter(|a| !matches!(a, Action::ActivateMana { .. }))
                .count()
                >= 2
        }
        DecisionKind::ChooseTarget { .. }
        | DecisionKind::DeclareAttacker { .. }
        | DecisionKind::DeclareBlocker { .. }
        | DecisionKind::Mulligan { .. }
        | DecisionKind::Discard { .. }
        | DecisionKind::ChooseMode { .. }
        | DecisionKind::ChooseName { .. }
        | DecisionKind::ChoosePlayer { .. } => d.option_count() >= 2,
        DecisionKind::ChooseCard { purpose, .. } => {
            matches!(
                purpose,
                CardPurpose::Search
                    | CardPurpose::Take
                    | CardPurpose::Discard
                    | CardPurpose::Sacrifice
                    | CardPurpose::Exile
            ) && d.option_count() >= 2
        }
        DecisionKind::Optional { kind, .. } => matches!(
            kind,
            OptionalKind::May
                | OptionalKind::IfDoes
                | OptionalKind::PayUnless
                | OptionalKind::Kicker
        ),
        _ => false,
    }
}

/// Builds a policy from a short spec: `random`, `random:<pass‰>`, `passive`, `greedy`.
pub fn from_spec(spec: &str) -> Result<Box<dyn Policy>, String> {
    let (kind, arg) = spec.split_once(':').unwrap_or((spec, ""));
    match kind {
        "random" if arg.is_empty() => Ok(Box::new(RandomPolicy::default())),
        "random" => arg
            .parse()
            .map(|p| Box::new(RandomPolicy::new(p)) as Box<dyn Policy>)
            .map_err(|_| format!("bad pass bias {arg:?}; use e.g. random:500")),
        "passive" => Ok(Box::new(PassivePolicy)),
        "greedy" => Ok(Box::new(GreedyPolicy::default())),
        other => Err(format!(
            "unknown policy {other:?}; known: random, random:<pass‰>, passive, greedy"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs() {
        assert!(from_spec("random").is_ok());
        assert!(from_spec("random:900").is_ok());
        assert!(from_spec("passive").is_ok());
        assert!(from_spec("greedy").is_ok());
        assert!(from_spec("clever").is_err());
        assert!(from_spec("random:x").is_err());
    }
}
