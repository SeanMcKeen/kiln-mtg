//! Coverage: whether the engine can play a card, and how much we trust it.

use mtg_core::OracleId;
use serde::{Deserialize, Serialize};

/// A part of a card the front-ends couldn't handle, with the reason.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UnsupportedSpan {
    /// Face index for multi-face cards; `None` when the whole card is affected.
    pub face: Option<u8>,
    /// The Oracle text span, when the problem is localized to some text.
    pub text: Option<String>,
    pub reason: String,
}

impl UnsupportedSpan {
    pub fn whole_card(reason: impl Into<String>) -> Self {
        Self {
            face: None,
            text: None,
            reason: reason.into(),
        }
    }
}

/// Support status of a card (`docs/06 Card Data/Card Data Model.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Coverage {
    /// Some text couldn't be handled. Experiments refuse these cards.
    Unsupported { reasons: Vec<UnsupportedSpan> },
    /// Has IR, not yet verified by tests.
    Parsed,
    /// Every ability has at least one passing scenario test.
    Verified { tests: Vec<String>, since: String },
}

/// The coarse level, for counting and filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageLevel {
    Unsupported,
    Parsed,
    Verified,
}

impl Coverage {
    pub fn level(&self) -> CoverageLevel {
        match self {
            Coverage::Unsupported { .. } => CoverageLevel::Unsupported,
            Coverage::Parsed => CoverageLevel::Parsed,
            Coverage::Verified { .. } => CoverageLevel::Verified,
        }
    }

    pub fn is_verified(&self) -> bool {
        matches!(self, Coverage::Verified { .. })
    }
}

/// Where a definition came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Provenance {
    /// No front-end has produced a definition yet.
    None,
    Parser {
        grammar_version: String,
    },
    Override {
        file: String,
        author: String,
    },
    Escape {
        fn_name: String,
    },
}

/// A card's behaviour definition. In M1 there is no IR yet, only provenance
/// and coverage; the `faces` IR arrives with ADR-0007 in M3.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardDefinition {
    pub oracle_id: OracleId,
    pub provenance: Provenance,
    pub coverage: Coverage,
}

/// The reason every card carries until a card-definition front-end exists.
pub const NO_FRONT_END_YET: &str = "no card-definition front-end yet (milestone M3, ADR-0007)";

impl CardDefinition {
    /// The M1 definition: unsupported, with `extra` reasons (e.g. unknown layout)
    /// added after the standard one.
    pub fn unsupported(
        oracle_id: OracleId,
        extra: impl IntoIterator<Item = UnsupportedSpan>,
    ) -> Self {
        let mut reasons = vec![UnsupportedSpan::whole_card(NO_FRONT_END_YET)];
        reasons.extend(extra);
        Self {
            oracle_id,
            provenance: Provenance::None,
            coverage: Coverage::Unsupported { reasons },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_with_status_tag() {
        let c = Coverage::Unsupported {
            reasons: vec![UnsupportedSpan::whole_card("x")],
        };
        let json = serde_json::to_string(&c).unwrap_or_default();
        assert!(json.starts_with(r#"{"status":"unsupported""#), "{json}");
        assert_eq!(c.level(), CoverageLevel::Unsupported);
    }
}
