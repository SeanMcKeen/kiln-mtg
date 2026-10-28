//! Live play (`docs/14 Players & Policies/C - LLM Live Decisions.md`, M10
//! plan L4): a game in which one seat's key decisions come from outside (an
//! MCP client, the LLM adapter's tests), one at a time. The other seats play
//! their spec policies; the caller's trivial decisions (payments, ordering,
//! a lone pass) go to greedy unless `ask_all`.

use std::collections::BTreeMap;
use std::sync::Arc;

use kiln_engine::rng::splitmix64;
use kiln_engine::{Game, GameResult, GameSetup, PlayerView, Recorder, Replay};
use kiln_policy::{GreedyPolicy, Policy, is_key_decision};
use kiln_types::PlayerId;
use serde_json::{Value, json};

use crate::experiment::Experiment;

/// A card's printed text, for payloads (agents hallucinate card text
/// without it, [[C - LLM Live Decisions]]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardText {
    pub mana_cost: Option<String>,
    pub type_line: String,
    pub oracle_text: String,
}

/// Looks a card's text up by name.
pub type Oracle<'a> = &'a dyn Fn(&str) -> Option<CardText>;

/// A game with one seat played from outside.
pub struct LiveGame {
    game: Game,
    setup: GameSetup,
    decks: Vec<String>,
    me: PlayerId,
    policies: Vec<Box<dyn Policy>>,
    fallback: GreedyPolicy,
    ask_all: bool,
    recorder: Recorder,
    /// Events already shown to the caller.
    shown: usize,
    /// Decisions the caller answered.
    pub answered: u32,
}

impl std::fmt::Debug for LiveGame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveGame")
            .field("decks", &self.decks)
            .field("me", &self.me)
            .finish_non_exhaustive()
    }
}

fn seat(p: usize) -> PlayerId {
    PlayerId(u8::try_from(p).unwrap_or(u8::MAX))
}

impl LiveGame {
    /// Deals game `index` of `exp` (its seating and seed), with deck `deck`'s
    /// seat played from outside, and plays up to that seat's first question.
    pub fn start(exp: &Experiment, deck: &str, index: u64, ask_all: bool) -> Result<Self, String> {
        let config = exp
            .configs()
            .first()
            .ok_or_else(|| "the spec has no configuration".to_owned())?;
        let (mut setup, seats) = exp.setup(config, index);
        setup.rules.log_events = true;
        let decks: Vec<String> = seats.iter().map(|d| exp.deck_id(*d).to_owned()).collect();
        let me = decks
            .iter()
            .position(|d| d == deck)
            .ok_or_else(|| format!("no deck {deck:?} in this pod; decks: {}", decks.join(", ")))?;
        let game = Game::new(Arc::clone(exp.pool()), &setup).map_err(|e| e.to_string())?;
        let mut policies = exp.policies(&seats);
        for (i, p) in policies.iter_mut().enumerate() {
            p.begin_game(seat(i), splitmix64(setup.seed ^ (i as u64 + 1)));
        }
        let mut live = Self {
            game,
            setup,
            decks,
            me: seat(me),
            policies,
            fallback: GreedyPolicy::default(),
            ask_all,
            recorder: Recorder::new(),
            shown: 0,
            answered: 0,
        };
        live.fallback.begin_game(live.me, 0);
        live.advance()?;
        Ok(live)
    }

    pub fn game(&self) -> &Game {
        &self.game
    }

    pub fn me(&self) -> PlayerId {
        self.me
    }

    /// Whether the caller has a decision to make.
    pub fn waiting(&self) -> bool {
        self.game
            .pending()
            .is_some_and(|d| self.game.decider_for(d.player) == self.me)
    }

    /// Plays every decision that isn't the caller's to make.
    fn advance(&mut self) -> Result<(), String> {
        let mut steps = 0u32;
        while let Some(d) = self.game.pending().cloned() {
            steps += 1;
            if steps > 500_000 {
                return Err("the game ran 500,000 decisions without asking".into());
            }
            let decider = self.game.decider_for(d.player);
            let count = d.option_count();
            if decider == self.me && count > 1 && (self.ask_all || is_key_decision(&d)) {
                return Ok(());
            }
            let (choice, why) = if count == 1 {
                (0, None)
            } else if decider == self.me {
                (self.fallback.decide(&self.game.view(decider), &d), None)
            } else {
                let p = &mut self.policies[decider.0 as usize];
                let c = p.decide(&self.game.view(decider), &d);
                (c, p.rationale())
            };
            self.recorder
                .apply(
                    &mut self.game,
                    choice.min(u32::try_from(count - 1).unwrap_or(0)),
                )
                .map_err(|e| e.to_string())?;
            if let Some(w) = why {
                self.recorder.annotate(format!("{decider}: {w}"));
            }
        }
        Ok(())
    }

    /// Answers the caller's decision with an option number or its exact
    /// label, notes the reason if given, and plays on to the next question.
    pub fn decide(&mut self, option: &str, rationale: Option<&str>) -> Result<(), String> {
        if !self.waiting() {
            return Err("no decision is waiting for you (the game is over)".into());
        }
        let n = self
            .game
            .pending()
            .map_or(0, kiln_engine::PendingDecision::option_count);
        let labels: Vec<String> = (0..n)
            .map(|i| self.game.option_label(i).unwrap_or_default())
            .collect();
        let i = match option.trim().parse::<usize>() {
            Ok(i) if i < n => i,
            Ok(i) => {
                return Err(format!(
                    "option {i} doesn't exist (0–{})",
                    n.saturating_sub(1)
                ));
            }
            Err(_) => labels
                .iter()
                .position(|l| l == option.trim())
                .ok_or_else(|| format!("no option {option:?}; options: {}", labels.join(" | ")))?,
        };
        self.recorder
            .apply(&mut self.game, u32::try_from(i).unwrap_or(0))
            .map_err(|e| e.to_string())?;
        self.answered += 1;
        if let Some(r) = rationale.filter(|r| !r.trim().is_empty()) {
            self.recorder
                .annotate(format!("{} (live): {}", self.me, r.trim()));
        }
        self.advance()
    }

    /// Lets the seat's own spec policy (search, a playbook…) answer the
    /// caller's next `count` decisions (0: the rest of the game), keeping its
    /// reasons in the replay. For handing the routine part of a game back.
    pub fn auto(&mut self, count: u32) -> Result<u32, String> {
        let me = self.me.0 as usize;
        let mut done = 0;
        while self.waiting() && (count == 0 || done < count) {
            let Some(d) = self.game.pending().cloned() else {
                break;
            };
            let p = &mut self.policies[me];
            let c = p.decide(&self.game.view(self.me), &d);
            let why = p.rationale();
            self.recorder
                .apply(
                    &mut self.game,
                    c.min(u32::try_from(d.option_count().saturating_sub(1)).unwrap_or(0)),
                )
                .map_err(|e| e.to_string())?;
            if let Some(w) = why {
                self.recorder.annotate(format!("{} (auto): {w}", self.me));
            }
            done += 1;
            self.advance()?;
        }
        Ok(done)
    }

    /// Why the caller can't take an action now (`Cast <card>`, `Play <land>`,
    /// `Activate <card> ability <n>`, or a card name): the engine's reasons
    /// with CR rules, or that it's available.
    pub fn why_not(&self, what: &str) -> String {
        let Some(action) = self.game.action_for(self.me, what) else {
            return format!(
                "{what}: you have no card or ability by that name in your hand, on the battlefield, in your graveyard, exile or the command zone"
            );
        };
        let reasons = self.game.why_not(self.me, &action);
        if reasons.is_empty() {
            return format!("{what}: available now");
        }
        let r: Vec<String> = reasons
            .iter()
            .map(|e| format!("CR {}: {}", e.cr, e.reason))
            .collect();
        format!("{what}: {}", r.join("; "))
    }

    /// The game so far as an annotated replay.
    pub fn replay(&self) -> Replay {
        self.recorder.clone().finish(&self.game, self.setup.clone())
    }

    /// What the caller sees ([`view_payload`]), and the events since the
    /// last payload.
    pub fn payload(&mut self, oracle: Oracle<'_>, detailed: bool) -> Value {
        let view = self.game.view(self.me);
        let v = view_payload(&view, &self.decks, oracle, detailed, self.shown);
        self.shown = view.event_count();
        v
    }
}

/// A decision payload for the viewer of `view` (M10 plan L4; [[C - LLM Live
/// Decisions]]): the pending decision if it's theirs (kind, rule, options),
/// the board as they see it, what happened since event `since` (at most the
/// last 40), and the text of the cards it names. `detailed` adds graveyards,
/// exile and the text of every visible card. Shared by live play and the LLM
/// adapter.
pub fn view_payload(
    view: &PlayerView<'_>,
    decks: &[String],
    oracle: Oracle<'_>,
    detailed: bool,
    since: usize,
) -> Value {
    let me = view.me();
    let mut named: Vec<String> = Vec::new();
    let name_of = |id: &kiln_types::ObjectId, named: &mut Vec<String>| -> Option<String> {
        let n = view.object(*id)?.name;
        if !named.contains(&n) {
            named.push(n.clone());
        }
        Some(n)
    };
    let decision = view.pending().map(|d| {
        let options: Vec<Value> = (0..d.option_count())
            .map(|i| json!({ "index": i, "label": view.option_label(i).unwrap_or_default() }))
            .collect();
        json!({
            "kind": decision_kind_name(&d.kind),
            "cr": d.cr.to_string(),
            "options": options,
        })
    });
    let mut players = Vec::new();
    for p in 0..view.player_count() {
        let pid = seat(p);
        let pub_ = view.player(pid);
        let battlefield: Vec<Value> = view
            .battlefield()
            .iter()
            .filter_map(|id| view.object(*id).filter(|o| o.controller == pid))
            .map(|o| {
                let _ = name_of(&o.id, &mut named);
                json!({
                    "name": o.name,
                    "tapped": o.tapped,
                    "power": o.power,
                    "toughness": o.toughness,
                    "counters": o.counters.iter().map(|(k, n)| format!("{n} {k:?}")).collect::<Vec<_>>(),
                })
            })
            .collect();
        let mut entry = json!({
            "player": pid.to_string(),
            "deck": decks.get(p).cloned().unwrap_or_default(),
            "you": pid == me,
            "in_game": pub_.in_game,
            "won": matches!(view.result(), Some(GameResult::Win { player, .. }) if *player == pid),
            "life": pub_.life,
            "hand": pub_.hand_count,
            "library": pub_.library_count,
            "battlefield": battlefield,
        });
        if detailed {
            let gy: Vec<String> = view
                .graveyard(pid)
                .iter()
                .filter_map(|id| name_of(id, &mut named))
                .collect();
            entry["graveyard"] = json!(gy);
        }
        players.push(entry);
    }
    let hand: Vec<String> = view
        .hand()
        .iter()
        .filter_map(|id| name_of(id, &mut named))
        .collect();
    let stack: Vec<Value> = view
        .stack()
        .iter()
        .rev()
        .map(|s| {
            if !named.contains(&s.name) {
                named.push(s.name.clone());
            }
            json!({
                "name": s.name,
                "controller": s.controller.to_string(),
                "ability": s.ability.map(|a| a + 1),
            })
        })
        .collect();
    let total = view.event_count();
    let recent: Vec<String> = view
        .events_since(since.max(total.saturating_sub(40)))
        .iter()
        .filter(|e| {
            !e.text.contains(" adds ")
                && !e.text.ends_with(" taps")
                && !e.text.starts_with("a card moves")
        })
        .map(|e| format!("{} (CR {})", e.text, e.cr))
        .collect();
    // Text for what the decision is about first, then everything named.
    let mut cards = BTreeMap::new();
    if !detailed {
        named.retain(|n| {
            hand.contains(n)
                || stack.iter().any(|s| s["name"] == n.as_str())
                || decision
                    .as_ref()
                    .is_some_and(|d| d["options"].to_string().contains(n.as_str()))
        });
    }
    for n in &named {
        if let Some(c) = oracle(n) {
            cards.insert(
                n.clone(),
                json!({ "cost": c.mana_cost, "type": c.type_line, "text": c.oracle_text }),
            );
        }
    }
    let t = view.turn();
    json!({
        "turn": t.number,
        "active": t.active.to_string(),
        "step": format!("{:?}", t.step),
        "you": me.to_string(),
        "decision": decision,
        "result": view.result().map(|r| match r {
            GameResult::Win { player, turn } => format!("{player} wins on turn {turn}"),
            GameResult::Draw { turn } => format!("a draw on turn {turn}"),
            GameResult::Aborted { reason, turn } => format!("aborted on turn {turn}: {reason:?}"),
        }),
        "your_hand": hand,
        "stack_top_first": stack,
        "players": players,
        "since_last": recent,
        "cards": cards,
        "exile": if detailed {
            json!(view.exile().iter().filter_map(|id| view.object(*id).map(|o| o.name)).collect::<Vec<_>>())
        } else {
            Value::Null
        },
    })
}

/// The scenario format's name for a decision kind (`priority`, `choose_target`, …).
fn decision_kind_name(k: &kiln_engine::DecisionKind) -> String {
    let s = format!("{k:?}");
    let head = s.split([' ', '{', '(']).next().unwrap_or("");
    let mut out = String::new();
    for (i, c) in head.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}
