//! `mtg-policy`: decision-makers (`docs/14 Players & Policies/Policy Interface.md`).
//!
//! Every decision-maker implements [`Policy`] and sees only a
//! [`PlayerView`], never the game state (FR-P6). Policies are deterministic
//! given their seed, which comes from a stream separate from the game's
//! (ADR-0005), so changing one seat's policy never changes another's shuffles.

use std::collections::VecDeque;

use mtg_core::PlayerId;
use mtg_engine::rng::GameRng;
use mtg_engine::{DecisionKind, PendingDecision, PlayerView};

/// A decision-maker for one seat.
pub trait Policy: Send {
    /// A short name for reports.
    fn name(&self) -> String;

    /// Called once per game before the first decision.
    fn begin_game(&mut self, _seat: PlayerId, _seed: u64) {}

    /// Returns the index of the chosen option. It must be `< decision.option_count()`.
    fn decide(&mut self, view: &PlayerView<'_>, decision: &PendingDecision) -> u32;
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

/// Builds a policy from a short spec: `random`, `random:<pass‰>`, `passive`.
pub fn from_spec(spec: &str) -> Result<Box<dyn Policy>, String> {
    let (kind, arg) = spec.split_once(':').unwrap_or((spec, ""));
    match kind {
        "random" if arg.is_empty() => Ok(Box::new(RandomPolicy::default())),
        "random" => arg
            .parse()
            .map(|p| Box::new(RandomPolicy::new(p)) as Box<dyn Policy>)
            .map_err(|_| format!("bad pass bias {arg:?}; use e.g. random:500")),
        "passive" => Ok(Box::new(PassivePolicy)),
        other => Err(format!(
            "unknown policy {other:?}; known: random, random:<pass‰>, passive"
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
        assert!(from_spec("greedy").is_err());
        assert!(from_spec("random:x").is_err());
    }
}
