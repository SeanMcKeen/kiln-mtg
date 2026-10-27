//! Finds `CR x.y` citations in source and docs and checks them against the
//! rule tree (`mtg dev cr-check`).
//!
//! Recognized forms, all prefixed by `CR `:
//! - single references: `CR 603.3b`, `CR 117`
//! - letter ranges: `CR 601.2a–i`, `CR 510.1c-d` (both endpoints are checked)
//! - full ranges: `CR 107.5–107.6`
//! - slash alternatives: `CR 903.9a/b`
//! - lists: `CR 101.4, 723`, `CR 508.1c and 509.1c`
//!
//! `CR 800-series` style phrases are treated as the bare rule (`800`).

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use mtg_core::CrRef;
use regex::Regex;
use serde::Serialize;

use crate::cr::RuleTree;

/// One rule reference found in a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Citation {
    pub file: PathBuf,
    pub line: usize,
    /// The citation text as written, e.g. `601.2a–i`.
    pub text: String,
    /// Every rule the citation names (both endpoints for a range).
    pub refs: Vec<String>,
}

/// A citation that doesn't match the CR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CitationProblem {
    pub citation: Citation,
    pub problem: String,
}

const REF: &str = r"\d{3}(?:\.\d+[a-z]?)?\b";
static LIST: LazyLock<Regex> = LazyLock::new(|| {
    let one = format!(r"{REF}(?:\s*[–-]\s*(?:{REF}|[a-z]\b))?(?:/[a-z]\b)*");
    re(&format!(
        r"\bCR[ \u{{a0}}]+({one}(?:(?:\s*[,+/]\s*|\s+(?:and|or)\s+){one})*)"
    ))
});
static ONE: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"({REF})(?:\s*[–-]\s*({REF}|[a-z]\b))?((?:/[a-z]\b)*)"
    ))
});

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|e| panic!("static regex {pattern:?} is valid: {e}"))
}

/// Scans text for citations. `file` is only used to label results.
pub fn scan_text(file: &Path, text: &str) -> Vec<Citation> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        for list in LIST.captures_iter(line) {
            for one in ONE.captures_iter(&list[1]) {
                let start = one[1].to_owned();
                let mut refs = vec![start.clone()];
                if let Some(end) = one.get(2) {
                    refs.push(expand_end(&start, end.as_str()));
                }
                if let Some(alts) = one.get(3) {
                    for letter in alts.as_str().split('/').filter(|s| !s.is_empty()) {
                        refs.push(expand_end(&start, letter));
                    }
                }
                out.push(Citation {
                    file: file.to_owned(),
                    line: i + 1,
                    text: one[0].to_owned(),
                    refs,
                });
            }
        }
    }
    out
}

/// `("601.2a", "i")` → `601.2i`; `("107.5", "107.6")` → `107.6`.
fn expand_end(start: &str, end: &str) -> String {
    if end.len() == 1 && end.chars().all(|c| c.is_ascii_lowercase()) {
        let stem = start.trim_end_matches(|c: char| c.is_ascii_lowercase());
        format!("{stem}{end}")
    } else {
        end.to_owned()
    }
}

/// Checks citations against the tree. Returns only the problems.
pub fn check(tree: &RuleTree, citations: &[Citation]) -> Vec<CitationProblem> {
    let mut problems = Vec::new();
    for c in citations {
        let parsed: Vec<Result<CrRef, String>> = c
            .refs
            .iter()
            .map(|r| r.parse::<CrRef>().map_err(|e| e.to_string()))
            .collect();
        for (r, p) in c.refs.iter().zip(&parsed) {
            match p {
                Err(e) => problems.push(CitationProblem {
                    citation: c.clone(),
                    problem: e.clone(),
                }),
                Ok(id) if !tree.contains(*id) => problems.push(CitationProblem {
                    citation: c.clone(),
                    problem: format!(
                        "rule {r} doesn't exist in the CR ({})",
                        tree.effective.trim_end_matches('.')
                    ),
                }),
                Ok(_) => {}
            }
        }
        if let [Ok(a), Ok(b), ..] = parsed.as_slice()
            && c.text.contains(['–', '-'])
            && b < a
        {
            problems.push(CitationProblem {
                citation: c.clone(),
                problem: format!("range end {b} comes before start {a}"),
            });
        }
    }
    problems
}

/// Walks `roots`, scanning `.rs`, `.md`, `.yaml`/`.yml` and `.toml` files.
/// Skips `target`, `.git`, `.claude`, `node_modules` and any `_external` directory.
pub fn scan_tree(roots: &[PathBuf]) -> std::io::Result<Vec<Citation>> {
    let mut files = Vec::new();
    for root in roots {
        collect(root, &mut files)?;
    }
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        out.extend(scan_text(&f, &text));
    }
    Ok(out)
}

/// Scans `roots` and checks every citation against `tree`.
pub fn check_tree(
    tree: &RuleTree,
    roots: &[PathBuf],
) -> std::io::Result<(Vec<Citation>, Vec<CitationProblem>)> {
    let cites = scan_tree(roots)?;
    let problems = check(tree, &cites);
    Ok((cites, problems))
}

fn collect(path: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.is_file() {
        if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("rs" | "md" | "yaml" | "yml" | "toml")
        ) {
            files.push(path.to_owned());
        }
        return Ok(());
    }
    if !meta.is_dir() {
        return Ok(()); // symlinks (e.g. docs/_external in a worktree) are not followed
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if matches!(
        name,
        "target" | ".git" | ".claude" | "node_modules" | "_external" | ".obsidian"
    ) {
        return Ok(());
    }
    for entry in std::fs::read_dir(path)? {
        collect(&entry?.path(), files)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(text: &str) -> Vec<Vec<String>> {
        scan_text(Path::new("t"), text)
            .into_iter()
            .map(|c| c.refs)
            .collect()
    }

    #[test]
    fn single_and_list_forms() {
        assert_eq!(refs("see CR 603.3b."), vec![vec!["603.3b"]]);
        assert_eq!(refs("(CR 101.4, 723)"), vec![vec!["101.4"], vec!["723"]]);
        assert_eq!(
            refs("CR 508.1c and 509.1c"),
            vec![vec!["508.1c"], vec!["509.1c"]]
        );
    }

    #[test]
    fn ranges_and_slashes() {
        assert_eq!(refs("CR 601.2a–i"), vec![vec!["601.2a", "601.2i"]]);
        assert_eq!(refs("CR 510.1c-d"), vec![vec!["510.1c", "510.1d"]]);
        assert_eq!(refs("CR 107.5–107.6"), vec![vec!["107.5", "107.6"]]);
        assert_eq!(refs("CR 903.9a/b"), vec![vec!["903.9a", "903.9b"]]);
        assert_eq!(
            refs("CR 601.2g–h, 605.3a"),
            vec![vec!["601.2g", "601.2h"], vec!["605.3a"]]
        );
    }

    #[test]
    fn ignores_non_citations() {
        assert_eq!(refs("CR 800-series"), vec![vec!["800"]]);
        assert!(refs("the CR is effective 2026-06-19").is_empty());
        assert!(refs("CRAB 603.3").is_empty());
        assert!(refs("CR 2026").is_empty());
    }
}
