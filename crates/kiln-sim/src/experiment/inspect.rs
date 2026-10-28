//! Looking at runs after the fact: comparing two runs, and playing one game
//! of a run again with its event log (shared by `kiln sim compare`,
//! `kiln game inspect` and the MCP tools).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use kiln_data::CardDb;
use kiln_engine::{CardPool, Event, Game, Target, ZoneId};
use kiln_protocol::GameRecord;
use kiln_protocol::report::Rate;
use kiln_types::ObjectId;
use serde::Serialize;

use super::{Experiment, ExperimentError, read_manifest, read_records, report, stats, store};
use crate::library::Library;

/// One deck's win rate in two runs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeckDiff {
    pub deck: String,
    pub a: Rate,
    pub b: Rate,
    /// b − a.
    pub difference: f64,
    /// Two-proportion z-test.
    pub p_value: f64,
    pub intervals_overlap: bool,
}

/// Two runs' base configurations, deck by deck.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Comparison {
    pub a: String,
    pub b: String,
    pub same_card_db: bool,
    pub decks: Vec<DeckDiff>,
    pub note: String,
}

/// Compares two runs (rebuilding both reports from their records).
pub fn compare(a: &Path, b: &Path) -> Result<Comparison, ExperimentError> {
    let (ra, rb) = (report::write_report(a)?, report::write_report(b)?);
    let rates = |r: &kiln_protocol::Report| {
        r.configs
            .first()
            .and_then(|c| c.win_rates.clone())
            .map(|w| w.decks)
            .unwrap_or_default()
    };
    let (da, db) = (rates(&ra), rates(&rb));
    let decks = da
        .iter()
        .filter_map(|x| {
            let y = db.iter().find(|y| y.deck == x.deck)?;
            Some(DeckDiff {
                deck: x.deck.clone(),
                difference: stats::r4(y.win.rate - x.win.rate),
                p_value: stats::r4(stats::two_proportion_p(x.win.k, x.win.n, y.win.k, y.win.n)),
                intervals_overlap: x.win.ci95[0] <= y.win.ci95[1] && y.win.ci95[0] <= x.win.ci95[1],
                a: x.win.clone(),
                b: y.win.clone(),
            })
        })
        .collect();
    Ok(Comparison {
        a: ra.run_id,
        b: rb.run_id,
        same_card_db: ra.versions.card_db == rb.versions.card_db,
        decks,
        note: "independent runs: a difference can come from any change between them; for a card's effect use mode: ab (paired seeds)".into(),
    })
}

/// One game of a run, played again.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Inspection {
    pub record: GameRecord,
    /// Whether it played differently than the run recorded (a determinism bug).
    pub diverged: bool,
    /// Notable events, with card names.
    pub events: Vec<String>,
}

/// Plays game `index` of `config` in run `dir` again with the event log on.
/// `last` keeps only the last events (0 = all).
pub fn inspect(
    dir: &Path,
    index: u64,
    config: &str,
    last: usize,
    db: &CardDb,
    lib: &Library,
) -> Result<Inspection, ExperimentError> {
    let manifest = read_manifest(dir)?;
    let spec_path = dir.join(store::SPEC);
    let text = std::fs::read_to_string(&spec_path)
        .map_err(|e| ExperimentError::Run(format!("{}: {e}", spec_path.display())))?;
    let spec_dir = manifest
        .spec_dir
        .clone()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let exp = Experiment::from_text(&text, &spec_dir, db, lib)?;
    if exp.resolved.hash != manifest.spec_hash {
        return Err(ExperimentError::Run(
            "the run's spec changed since it ran".into(),
        ));
    }
    let Some(c) = exp.configs().iter().find(|c| c.name == config) else {
        return Err(ExperimentError::Run(format!(
            "no configuration {config:?} in this run"
        )));
    };
    if index >= exp.games() {
        return Err(ExperimentError::Run(format!(
            "game {index} is out of range: the run has games 0 to {}",
            exp.games().saturating_sub(1)
        )));
    }
    let stored = read_records(dir)?
        .into_iter()
        .find(|r| r.config == config && r.index == index);
    let Some((played, game)) = exp.play_logged(c, index) else {
        return Err(ExperimentError::Run(format!(
            "game {index} couldn't be played"
        )));
    };
    let diverged = stored
        .as_ref()
        .is_some_and(|s| s.winner != played.record.winner || s.turns != played.record.turns);
    let events = game.map_or_else(Vec::new, |g| describe_events(&g, exp.pool()));
    let from = if last == 0 {
        0
    } else {
        events.len().saturating_sub(last)
    };
    Ok(Inspection {
        record: played.record,
        diverged,
        events: events[from..].to_vec(),
    })
}

/// One line per notable event, with card names.
pub fn describe_events(game: &Game, pool: &CardPool) -> Vec<String> {
    let mut names: BTreeMap<ObjectId, String> = BTreeMap::new();
    let mut out = Vec::new();
    let name = |id: &ObjectId, names: &BTreeMap<ObjectId, String>| {
        names.get(id).cloned().unwrap_or_else(|| format!("{id}"))
    };
    for e in game.events() {
        let line = match e {
            Event::TurnBegan { number, active } => Some(format!("— turn {number} ({active}) —")),
            Event::ZoneChange {
                def,
                new,
                from,
                to,
                controller,
                ..
            } => {
                let n = pool.get(*def).name.clone();
                names.insert(*new, n.clone());
                match (from, to) {
                    (ZoneId::Battlefield, ZoneId::Graveyard(_)) => {
                        Some(format!("  {n} ({controller}) is put into a graveyard"))
                    }
                    (ZoneId::Battlefield, ZoneId::Exile) => {
                        Some(format!("  {n} ({controller}) is exiled"))
                    }
                    (ZoneId::Stack, ZoneId::Battlefield) => {
                        Some(format!("  {n} enters under {controller}'s control"))
                    }
                    _ => None,
                }
            }
            Event::SpellCast { player, def, .. } => {
                Some(format!("  {player} casts {}", pool.get(*def).name))
            }
            Event::LandPlayed { player, object } => {
                Some(format!("  {player} plays {}", name(object, &names)))
            }
            Event::AbilityActivated {
                player,
                def,
                ability,
                ..
            } => Some(format!(
                "  {player} activates {} ability {}",
                pool.get(*def).name,
                ability + 1
            )),
            Event::Countered { object } => Some(format!("  {} is countered", name(object, &names))),
            Event::Damage {
                source,
                target,
                amount,
                combat,
            } => Some(format!(
                "  {} deals {amount}{} damage to {}",
                name(source, &names),
                if *combat { " combat" } else { "" },
                match target {
                    Target::Player(p) => p.to_string(),
                    Target::Object(o) => name(o, &names),
                    Target::StackAbility { .. } => "an ability".into(),
                }
            )),
            Event::LifeLost { player, amount } => Some(format!("  {player} loses {amount} life")),
            Event::PlayerLost { player, reason } => {
                Some(format!("  {player} loses the game ({reason:?})"))
            }
            Event::WonByEffect { player } => Some(format!("  {player} wins the game by an effect")),
            Event::ShortcutTaken { .. } | Event::LoopDraw => Some(format!("  {e:?}")),
            _ => None,
        };
        if let Some(l) = line {
            out.push(l);
        }
    }
    out
}
