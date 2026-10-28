//! Checking a playbook (M8 plan P5; B - Playbooks, "Validation workflow"):
//! compile it, then play goldfish games with it and report which rules
//! fired, how often it deferred, and any runtime errors.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use kiln_data::CardDb;
use kiln_protocol::Outcome;
use serde::Serialize;

use super::{Experiment, ExperimentError};
use crate::library::Library;

/// What a playbook did in a dry run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlaybookCheck {
    pub playbook: String,
    /// The compile error, with line and column; nothing else runs then.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compile_error: Option<String>,
    /// Functions it defines that answer decisions.
    pub hooks: Vec<String>,
    pub games: u64,
    pub wins: u64,
    /// The deck's median own turn when it won, if it won.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median_win_turn: Option<u32>,
    /// `playbook:answered`, `fallback:answered`, `playbook:errors`, `tag:<name>`, …
    pub counters: BTreeMap<String, u64>,
    /// Share of the deck's decisions the playbook answered.
    pub playbook_share: f64,
    /// The first distinct runtime errors.
    pub errors: Vec<String>,
}

/// Compiles `playbook` and plays `games` goldfish games of `deck` with it
/// (greedy fallback, three passive opponents).
pub fn check_playbook(
    deck: &Path,
    playbook: &Path,
    games: u64,
    db: &CardDb,
    lib: &Library,
) -> Result<PlaybookCheck, ExperimentError> {
    let abs = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let (deck, book) = (abs(deck), abs(playbook));
    let name = playbook.file_name().map_or_else(
        || playbook.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let text = std::fs::read_to_string(&book)
        .map_err(|e| ExperimentError::Playbook(format!("can't read {}: {e}", book.display())))?;
    let compiled = match kiln_playbook::Playbook::compile(&name, &text) {
        Ok(b) => b,
        Err(e) => {
            return Ok(PlaybookCheck {
                playbook: name,
                compile_error: Some(e.to_string()),
                hooks: Vec::new(),
                games: 0,
                wins: 0,
                median_win_turn: None,
                counters: BTreeMap::new(),
                playbook_share: 0.0,
                errors: Vec::new(),
            });
        }
    };
    let spec = format!(
        "spec: kiln-spec/1\nname: playbook-check\nmode: goldfish\ngames: {games}\nseed: 1\n\
         decks: [{{ id: deck, source: {{ file: {deck:?} }} }}]\n\
         policies: {{ deck: {{ kind: playbook, playbook: {book:?}, fallback: {{ kind: greedy }} }} }}\n\
         goldfish: {{ opponents: 3 }}\nlimits: {{ max_turns: 60 }}\n\
         outputs: {{ replays: {{ keep: none }} }}\n",
        deck = deck.display().to_string(),
        book = book.display().to_string(),
    );
    let exp = Experiment::from_text(&spec, Path::new("."), db, lib)?;
    let never = AtomicBool::new(false);
    let config = &exp.configs()[0];
    let mut counters: BTreeMap<String, u64> = BTreeMap::new();
    let mut errors: Vec<String> = Vec::new();
    let mut win_turns: Vec<u32> = Vec::new();
    for i in 0..exp.games() {
        let Some(r) = exp.play_game(config, i, &never) else {
            continue;
        };
        let Some(seat) = r.seats.iter().position(|s| s == "deck") else {
            continue;
        };
        if let Some(s) = r.policy_stats.as_ref().and_then(|v| v.get(seat)) {
            for (k, n) in s {
                *counters.entry(k.clone()).or_default() += n;
            }
        }
        for e in r.policy_errors.iter().flatten() {
            if errors.len() < 10 && !errors.contains(e) {
                errors.push(e.clone());
            }
        }
        if r.outcome == Outcome::Win && r.winner == Some(seat) {
            let n = u32::try_from(r.seats.len()).unwrap_or(1);
            let s = u32::try_from(seat).unwrap_or(0);
            win_turns.push(if r.turns <= s {
                0
            } else {
                (r.turns - 1 - s) / n + 1
            });
        }
    }
    win_turns.sort_unstable();
    let answered = counters.get("playbook:answered").copied().unwrap_or(0);
    let fallback = counters.get("fallback:answered").copied().unwrap_or(0);
    Ok(PlaybookCheck {
        playbook: name,
        compile_error: None,
        hooks: compiled.hooks().iter().cloned().collect(),
        games: exp.games(),
        wins: win_turns.len() as u64,
        median_win_turn: (!win_turns.is_empty()).then(|| super::stats::quantile(&win_turns, 0.5)),
        counters,
        playbook_share: super::stats::r4(answered as f64 / (answered + fallback).max(1) as f64),
        errors,
    })
}
