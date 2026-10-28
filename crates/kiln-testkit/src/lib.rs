//! `kiln-testkit`: shared test tooling.
//!
//! M1 provides the machine-readable Comprehensive Rules ([`cr`]) and the
//! citation checker ([`citations`]) described in `docs/04 Rules Map/CR Structure.md`.
//! Scenario loading, golden replays and the fuzz harness arrive in M2.

pub mod citations;
pub mod cr;
pub mod puzzle;
pub mod real;
pub mod scenario;

pub use citations::{Citation, CitationProblem, check_tree, scan_text};
pub use cr::{Rule, RuleTree, RuleTreeError};
