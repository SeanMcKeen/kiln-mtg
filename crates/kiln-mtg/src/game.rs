//! `kiln game …`: replaying and inspecting single games
//! (`docs/07 Interfaces/CLI & REPL.md`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use kiln_engine::{CardPool, Event, Game, Replay, ZoneId};
use kiln_sim::experiment::{self, Experiment};
use kiln_types::ObjectId;

use crate::{DbArgs, card_library, print_json};

#[derive(Debug, Subcommand)]
pub enum GameCmd {
    /// Re-run a replay file (kiln-replay/1) and check it reproduces exactly.
    Replay {
        replay: PathBuf,
        #[command(flatten)]
        db: DbArgs,
        #[arg(long)]
        json: bool,
    },
    /// Play one game of a run again and show what happened.
    Inspect {
        /// The run directory.
        dir: PathBuf,
        /// The game index.
        index: u64,
        /// The configuration (`base`, or an A/B variant).
        #[arg(long, default_value = "base")]
        config: String,
        /// How many of the last events to show (0: all).
        #[arg(long, default_value_t = 60)]
        events: usize,
        #[command(flatten)]
        db: DbArgs,
        #[arg(long)]
        json: bool,
    },
}

pub fn game(cmd: GameCmd) -> Result<ExitCode> {
    match cmd {
        GameCmd::Replay { replay, db, json } => replay_file(&replay, &db, json),
        GameCmd::Inspect {
            dir,
            index,
            config,
            events,
            db,
            json,
        } => inspect(&dir, index, &config, events, &db, json),
    }
}

fn replay_file(path: &Path, db: &DbArgs, json: bool) -> Result<ExitCode> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let replay: Replay =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    let (db, _) = db.load()?;
    let lib = card_library(&db)?;
    let (pool, _) = lib.pool();
    match replay.run(Arc::new(pool)) {
        Ok(g) => {
            if json {
                print_json(&serde_json::json!({
                    "reproduced": true,
                    "result": g.result(),
                    "turns": g.turn().number,
                    "choices": replay.choices.len(),
                }))?;
            } else {
                println!(
                    "reproduced: {} choices, turn {}, result {:?}",
                    replay.choices.len(),
                    g.turn().number,
                    g.result()
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("error: the replay diverged: {e}");
            Ok(ExitCode::from(5))
        }
    }
}

fn inspect(
    dir: &Path,
    index: u64,
    config: &str,
    last: usize,
    db: &DbArgs,
    json: bool,
) -> Result<ExitCode> {
    let manifest = experiment::read_manifest(dir).map_err(|e| anyhow::anyhow!("{e}"))?;
    let spec_text = std::fs::read_to_string(dir.join(experiment::store::SPEC))
        .with_context(|| format!("reading the run's spec in {}", dir.display()))?;
    let spec_dir = manifest
        .spec_dir
        .clone()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let (db, _) = db.load()?;
    let lib = card_library(&db)?;
    let exp = Experiment::from_text(&spec_text, &spec_dir, &db, &lib)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if exp.resolved.hash != manifest.spec_hash {
        bail!("the run's spec hash changed; was it edited?");
    }
    let Some(c) = exp.configs().iter().find(|c| c.name == config) else {
        bail!("no configuration {config:?} in this run");
    };
    let stored = experiment::read_records(dir)
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .into_iter()
        .find(|r| r.config == config && r.index == index);
    let Some((played, game)) = exp.play_logged(c, index) else {
        bail!("game {index} couldn't be played");
    };
    if let Some(s) = &stored
        && (s.winner != played.record.winner || s.turns != played.record.turns)
    {
        eprintln!(
            "warning: this game played differently than the run recorded (determinism divergence)"
        );
    }
    let Some(game) = game else {
        bail!("game {index} failed to set up");
    };
    let lines = describe_events(&game, exp.pool());
    let from = if last == 0 {
        0
    } else {
        lines.len().saturating_sub(last)
    };
    if json {
        print_json(&serde_json::json!({ "record": played.record, "events": &lines[from..] }))?;
    } else {
        let r = &played.record;
        println!("game {index} ({config}), seed {:#x}", r.seed);
        for (seat, d) in r.seats.iter().enumerate() {
            let fate = if r.winner == Some(seat) {
                format!("won ({})", r.win_type.clone().unwrap_or_default())
            } else {
                r.eliminations.iter().find(|e| e.seat == seat).map_or_else(
                    || "still in".into(),
                    |e| format!("out on turn {} ({})", e.turn, e.reason),
                )
            };
            println!(
                "  seat {}: {d} — {} mulligan(s), {fate}",
                seat + 1,
                r.mulligans[seat]
            );
        }
        println!(
            "  {:?} after {} turns, {} decisions\n",
            r.outcome, r.turns, r.decisions
        );
        for l in &lines[from..] {
            println!("{l}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// One line per notable event, with card names.
fn describe_events(game: &Game, pool: &CardPool) -> Vec<String> {
    let mut names: BTreeMap<ObjectId, String> = BTreeMap::new();
    let mut out = Vec::new();
    let mut turn = 0;
    for e in game.events() {
        let name = |id: &ObjectId, names: &BTreeMap<ObjectId, String>| {
            names.get(id).cloned().unwrap_or_else(|| format!("{id}"))
        };
        let line = match e {
            Event::TurnBegan { number, active } => {
                turn = *number;
                Some(format!("— turn {number} ({active}) —"))
            }
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
                    (ZoneId::Battlefield, ZoneId::Graveyard(_)) => Some(format!(
                        "  {n} ({controller}) dies or is put into a graveyard"
                    )),
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
                    kiln_engine::Target::Player(p) => p.to_string(),
                    kiln_engine::Target::Object(o) => name(o, &names),
                    kiln_engine::Target::StackAbility { .. } => "an ability".into(),
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
    let _ = turn;
    out
}
