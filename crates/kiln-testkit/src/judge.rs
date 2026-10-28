//! The judge tool (`docs/07 Interfaces/Judge Tool UX.md`, M10 plan L3): load
//! a scenario, step through it, and ask why. Every answer comes from the
//! engine with a CR rule; nothing is generated.
//!
//! Commands (one per line):
//! - a number or an option label: choose it;
//! - `options`: the pending decision again;
//! - `why-not [P2] <label or card>`: why that action isn't available;
//! - `why <event #>`: an event, its rule and what caused it;
//! - `events [n]`: the last `n` events (default 20), numbered;
//! - `view`: life, zones and the stack, for every player;
//! - `branch` / `back`: save the game, or return to the last saved one;
//! - `result`.

use std::fmt::Write as _;
use std::sync::Arc;

use kiln_engine::{CardPool, Game, GameResult, ZoneId};
use kiln_types::PlayerId;

use crate::scenario::{self, Scenario};

/// A judge session: the current game and the saved branches.
#[derive(Debug)]
pub struct Judge {
    game: Game,
    saved: Vec<Game>,
}

fn player(p: PlayerId) -> String {
    p.to_string()
}

impl Judge {
    /// Sets the scenario up (running its script) and starts a session.
    pub fn load(s: &Scenario, pool: &Arc<CardPool>) -> Result<Self, String> {
        let game = scenario::run(s, pool).map_err(|e| e.to_string())?;
        Ok(Self {
            game,
            saved: Vec::new(),
        })
    }

    pub fn game(&self) -> &Game {
        &self.game
    }

    /// The pending decision with its rule and numbered options.
    pub fn prompt(&self) -> String {
        let g = &self.game;
        if let Some(r) = g.result() {
            return format!("the game is over: {}", describe_result(r));
        }
        let Some(d) = g.pending() else {
            return "no decision is pending".into();
        };
        let t = g.turn();
        let mut s = format!(
            "[turn {} ({}) · {:?} · {} decides: {} (CR {})]\n",
            t.number,
            player(t.active),
            t.step,
            player(g.decider_for(d.player)),
            scenario::kind_name(&d.kind),
            d.cr
        );
        for (i, l) in scenario::labels(g).iter().enumerate() {
            let _ = writeln!(s, "  {i}) {l}");
        }
        s
    }

    /// Runs one command; the reply is text for a person or an agent.
    pub fn command(&mut self, line: &str) -> Result<String, String> {
        let line = line.trim();
        let (verb, rest) = line.split_once(' ').unwrap_or((line, ""));
        match verb {
            "" | "options" => Ok(self.prompt()),
            "why-not" | "whynot" => self.why_not(rest),
            "why" => self.why(rest),
            "events" => Ok(self.events(rest.trim().parse().unwrap_or(20))),
            "view" => Ok(self.view()),
            "branch" => {
                self.saved.push(self.game.clone());
                Ok(format!("saved (branch {})", self.saved.len()))
            }
            "back" => match self.saved.pop() {
                Some(g) => {
                    self.game = g;
                    Ok(format!("back to the saved game\n{}", self.prompt()))
                }
                None => Err("nothing saved: `branch` first".into()),
            },
            "result" => Ok(self
                .game
                .result()
                .map_or_else(|| "in progress".into(), describe_result)),
            _ => self.choose(line),
        }
    }

    fn choose(&mut self, what: &str) -> Result<String, String> {
        let labels = scenario::labels(&self.game);
        if labels.is_empty() {
            return Err("no decision is pending".into());
        }
        let i = match what.parse::<usize>() {
            Ok(i) if i < labels.len() => i,
            Ok(i) => return Err(format!("option {i} doesn't exist (0–{})", labels.len() - 1)),
            Err(_) => labels.iter().position(|l| l == what).ok_or_else(|| {
                format!(
                    "unknown command or option {what:?}; options: {}",
                    labels.join(" | ")
                )
            })?,
        };
        let before = self.game.events().len();
        self.game
            .apply_option(u32::try_from(i).unwrap_or(0))
            .map_err(|e| e.to_string())?;
        let mut s = format!("chose {i}) {}\n", labels[i]);
        for e in self.game.describe_events().into_iter().skip(before) {
            let _ = writeln!(s, "  #{} {} (CR {})", e.index, e.text, e.cr);
        }
        s.push_str(&self.prompt());
        Ok(s)
    }

    fn why_not(&self, rest: &str) -> Result<String, String> {
        let g = &self.game;
        let (who, what) = match rest.split_once(' ') {
            Some((p, w)) if p.starts_with('P') && p[1..].parse::<u8>().is_ok() => (
                PlayerId(p[1..].parse::<u8>().unwrap_or(1).saturating_sub(1)),
                w,
            ),
            _ => (
                g.pending().map_or(PlayerId(0), |d| g.decider_for(d.player)),
                rest,
            ),
        };
        let action = g
            .action_for(who, what)
            .ok_or_else(|| format!("{} has no card or ability matching {what:?}", player(who)))?;
        let reasons = g.why_not(who, &action);
        if reasons.is_empty() {
            return Ok(format!(
                "{what}: {} can do that now (it's on the menu)",
                player(who)
            ));
        }
        let mut s = format!("{} can't {what} now:\n", player(who));
        for r in reasons {
            let _ = writeln!(s, "  - CR {}: {}", r.cr, r.reason);
        }
        Ok(s)
    }

    fn why(&self, rest: &str) -> Result<String, String> {
        let n: usize = rest
            .trim()
            .trim_start_matches('#')
            .parse()
            .map_err(|_| "usage: why <event number> (see `events`)".to_owned())?;
        let events = self.game.describe_events();
        let e = events.get(n).ok_or_else(|| format!("no event #{n}"))?;
        let mut s = format!("#{} {} (CR {})", e.index, e.text, e.cr);
        if let Some(c) = self.game.cause_of(n).and_then(|c| events.get(c)) {
            let _ = write!(s, "\n  because of #{} {} (CR {})", c.index, c.text, c.cr);
        }
        Ok(s)
    }

    fn events(&self, n: usize) -> String {
        let events = self.game.describe_events();
        let mut s = String::new();
        for e in events.iter().skip(events.len().saturating_sub(n)) {
            let _ = writeln!(s, "#{} {} (CR {})", e.index, e.text, e.cr);
        }
        s
    }

    fn view(&self) -> String {
        let g = &self.game;
        let name = |id: &kiln_types::ObjectId| g.object_name(*id).unwrap_or("?").to_owned();
        let mut s = String::new();
        for p in 0..g.player_count() {
            let pid = PlayerId(u8::try_from(p).unwrap_or(0));
            let pl = g.player_state(pid);
            let _ = writeln!(
                s,
                "{}: life {}, library {}, hand [{}], graveyard [{}]",
                player(pid),
                pl.life,
                pl.library.len(),
                pl.hand.iter().map(&name).collect::<Vec<_>>().join(", "),
                pl.graveyard
                    .iter()
                    .map(&name)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let perms: Vec<String> = g
                .objects_in(ZoneId::Battlefield)
                .iter()
                .filter(|id| g.object(**id).is_some_and(|o| o.controller == pid))
                .map(|id| {
                    let tapped = g.object(*id).is_some_and(|o| o.tapped);
                    format!("{}{}", name(id), if tapped { " (tapped)" } else { "" })
                })
                .collect();
            let _ = writeln!(s, "  battlefield: {}", perms.join(", "));
        }
        let stack: Vec<String> = g.objects_in(ZoneId::Stack).iter().map(&name).collect();
        let _ = writeln!(s, "stack (top last): {}", stack.join(", "));
        s
    }
}

fn describe_result(r: &GameResult) -> String {
    match r {
        GameResult::Win { player: p, turn } => format!("{} wins on turn {turn}", player(*p)),
        GameResult::Draw { turn } => format!("a draw on turn {turn}"),
        GameResult::Aborted { reason, turn } => format!("aborted on turn {turn}: {reason:?}"),
    }
}
