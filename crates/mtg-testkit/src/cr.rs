//! Parses the Comprehensive Rules text (`docs/_external/cr-lf.txt`) into a
//! rule tree.
//!
//! The text is normalized first (LF line endings, no BOM; see `CLAUDE.md`).
//! It has four parts: an introduction, a table of contents, the numbered rules,
//! and a glossary followed by credits. The table of contents also lists
//! "Glossary" and "Credits", so the numbered rules start after the *first*
//! "Credits" line and end at the *second* "Glossary" line.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;

use mtg_core::CrRef;
use regex::Regex;
use serde::Serialize;

/// One numbered rule or subrule, with any example lines appended to its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rule {
    pub id: CrRef,
    /// Section headers like `704. State-Based Actions` keep only the title.
    pub text: String,
    pub parent: Option<CrRef>,
    pub children: Vec<CrRef>,
}

/// The parsed rules, keyed by reference, plus the glossary.
#[derive(Debug, Clone, Default)]
pub struct RuleTree {
    /// The effective-date line, e.g. "These rules are effective as of June 19, 2026."
    pub effective: String,
    pub rules: BTreeMap<CrRef, Rule>,
    pub glossary: BTreeMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum RuleTreeError {
    #[error(
        "can't read the Comprehensive Rules at {path}: {source}. Download and normalize it with the commands in CLAUDE.md (\"Refreshing local reference data\")"
    )]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("the CR text doesn't have the expected structure: {0}")]
    Structure(String),
}

static RULE_HEADER: LazyLock<Regex> = LazyLock::new(|| re(r"^(\d{3})\. (.+)$"));
static SUBRULE: LazyLock<Regex> = LazyLock::new(|| re(r"^(\d{3}\.\d+)\. ?(.*)$"));
static LETTERED: LazyLock<Regex> = LazyLock::new(|| re(r"^(\d{3}\.\d+[a-z]) (.*)$"));

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|e| panic!("static regex {pattern:?} is valid: {e}"))
}

impl RuleTree {
    /// Loads and parses a normalized CR text file.
    pub fn load(path: &Path) -> Result<Self, RuleTreeError> {
        let text = std::fs::read_to_string(path).map_err(|source| RuleTreeError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::parse(&text)
    }

    /// Parses normalized CR text. Tolerates CRLF and a BOM anyway.
    pub fn parse(text: &str) -> Result<Self, RuleTreeError> {
        let text = text.trim_start_matches('\u{feff}');
        let lines: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).collect();
        let effective = lines
            .iter()
            .find(|l| l.starts_with("These rules are effective"))
            .map(|l| (*l).to_owned())
            .unwrap_or_default();
        let credits: Vec<usize> = positions(&lines, "Credits");
        let glossary: Vec<usize> = positions(&lines, "Glossary");
        let (Some(&body_start), Some(&body_end)) = (credits.first(), glossary.get(1)) else {
            return Err(RuleTreeError::Structure(
                "expected \"Credits\" in the contents and a second \"Glossary\" heading".into(),
            ));
        };
        let gloss_end = credits.get(1).copied().unwrap_or(lines.len());

        let mut tree = RuleTree {
            effective,
            ..Default::default()
        };
        let mut current: Option<CrRef> = None;
        for line in &lines[body_start + 1..body_end] {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parsed = if let Some(c) = LETTERED.captures(line) {
                Some((c[1].to_owned(), c[2].to_owned()))
            } else if let Some(c) = SUBRULE.captures(line) {
                Some((c[1].to_owned(), c[2].to_owned()))
            } else {
                RULE_HEADER
                    .captures(line)
                    .map(|c| (c[1].to_owned(), c[2].to_owned()))
            };
            match parsed {
                Some((id, body)) => {
                    let id: CrRef = id
                        .parse()
                        .map_err(|e| RuleTreeError::Structure(format!("{e}")))?;
                    tree.rules.insert(
                        id,
                        Rule {
                            id,
                            text: body,
                            parent: id.parent(),
                            children: Vec::new(),
                        },
                    );
                    current = Some(id);
                }
                None => {
                    // Section headings like "1. Game Concepts" and example lines.
                    if let Some(rule) = current.and_then(|id| tree.rules.get_mut(&id)) {
                        rule.text.push('\n');
                        rule.text.push_str(line);
                    }
                }
            }
        }
        let ids: Vec<CrRef> = tree.rules.keys().copied().collect();
        for id in ids {
            if let Some(parent) = id.parent()
                && let Some(p) = tree.rules.get_mut(&parent)
            {
                p.children.push(id);
            }
        }

        // Glossary: a term line followed by one or more definition lines, separated by blank lines.
        let mut block: Vec<&str> = Vec::new();
        for line in lines[body_end + 1..gloss_end]
            .iter()
            .chain(std::iter::once(&""))
        {
            if line.trim().is_empty() {
                if let Some((term, def)) = block.split_first() {
                    tree.glossary.insert((*term).to_owned(), def.join("\n"));
                }
                block.clear();
            } else {
                block.push(line.trim());
            }
        }

        if tree.rules.len() < 100 {
            return Err(RuleTreeError::Structure(format!(
                "only {} rules parsed; is this the Comprehensive Rules text?",
                tree.rules.len()
            )));
        }
        Ok(tree)
    }

    pub fn get(&self, id: CrRef) -> Option<&Rule> {
        self.rules.get(&id)
    }

    pub fn contains(&self, id: CrRef) -> bool {
        self.rules.contains_key(&id)
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

fn positions(lines: &[&str], needle: &str) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim() == needle)
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A miniature CR in the real file's shape (structure only; no CR text).
    pub(crate) const MINI: &str = "Title\n\nThese rules are effective as of June 19, 2026.\n\nContents\n\n1. Game Concepts\n100. General\n\nGlossary\n\nCredits\n\n\
1. Game Concepts\n\n100. General\n\n100.1. First rule.\n\n100.1a A subrule.\n\nExample: An example line.\n\n100.1b Another subrule.\n\n100.2. Second rule.\n\n\
7. Additional Rules\n\n704. Things\n\n704.5. List.\n\n704.5k Item k.\n\n704.5m Item m.\n\n\
Glossary\n\nAlpha\nFirst definition.\n\nBeta\nSecond definition.\nMore.\n\nCredits\n\nPeople\n";

    fn padded() -> String {
        // The parser rejects trees with < 100 rules, so pad with synthetic subrules.
        let mut s = MINI.replace(
            "100.2. Second rule.\n",
            &(2..120)
                .map(|i| format!("100.{i}. Rule {i}.\n\n"))
                .collect::<String>(),
        );
        s.push('\n');
        s
    }

    #[test]
    fn parses_structure() {
        let tree = RuleTree::parse(&padded()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            tree.effective,
            "These rules are effective as of June 19, 2026."
        );
        let r = tree
            .get(mtg_core::cr!("100.1a"))
            .unwrap_or_else(|| panic!("100.1a"));
        assert!(r.text.starts_with("A subrule."));
        assert!(r.text.contains("Example: An example line."));
        let parent = tree
            .get(mtg_core::cr!("100.1"))
            .unwrap_or_else(|| panic!("100.1"));
        assert_eq!(parent.children.len(), 2);
        assert!(tree.contains(mtg_core::cr!("704.5m")));
        assert!(!tree.contains(mtg_core::cr!("704.5n")));
        assert_eq!(
            tree.get(mtg_core::cr!("704")).map(|r| r.text.as_str()),
            Some("Things")
        );
        assert_eq!(
            tree.glossary.get("Beta").map(String::as_str),
            Some("Second definition.\nMore.")
        );
    }

    #[test]
    fn rejects_non_cr_text() {
        assert!(matches!(
            RuleTree::parse("hello"),
            Err(RuleTreeError::Structure(_))
        ));
    }
}
