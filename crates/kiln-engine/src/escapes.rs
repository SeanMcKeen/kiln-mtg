//! The escape registry (ADR-0007): named Rust implementations for effects
//! the IR can't express. A card definition refers to one with
//! `Effect::Escape { function, .. }`; a name that isn't registered here keeps
//! the card `unsupported` (`support.rs`), so nothing runs as a silent no-op.
//!
//! Adding an entry is a reviewed engine change, like adding an IR variant.
//! Each one starts a procedure the IR can't express and pushes frames for
//! its decisions (M6 6.6: Doomsday, Sylvan Library, Tainted Pact).

use crate::effects::EffectTask;
use crate::engine::Game;

/// An escape: runs as one effect step and may push frames onto the task.
pub(crate) type EscapeFn = fn(&mut Game, &mut EffectTask);

/// Registered escapes, sorted by name.
const ESCAPES: &[(&str, EscapeFn)] = &[
    ("doomsday", |g, t| g.doomsday(t)),
    ("suspend_haste", |g, t| g.suspend_haste(t)),
    ("suspend_upkeep", |g, t| g.suspend_upkeep(t)),
    ("sylvan_library", |g, t| g.sylvan_library(t)),
    ("tainted_pact", |g, t| g.tainted_pact(t)),
];

/// The implementation registered under `name`.
pub(crate) fn lookup(name: &str) -> Option<EscapeFn> {
    ESCAPES
        .binary_search_by(|(n, _)| (*n).cmp(name))
        .ok()
        .map(|i| ESCAPES[i].1)
}

/// Names of every registered escape.
pub fn registered() -> Vec<&'static str> {
    ESCAPES.iter().map(|(n, _)| *n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_sorted_and_unknown_names_miss() {
        assert!(ESCAPES.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(lookup("no_such_escape").is_none());
    }
}
