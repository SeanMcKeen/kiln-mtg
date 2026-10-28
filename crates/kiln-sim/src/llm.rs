//! The built-in LLM adapter (M10 plan L6; [[C - LLM Live Decisions]]): a
//! policy that asks Claude through the Anthropic Messages API.
//!
//! Each decision is one request: the same payload live play shows (the
//! decision and its rule, the options, the board, recent events, card text),
//! and a JSON reply `{ "option": <index>, "reason": "…" }` constrained by
//! structured outputs. With `ask: key` it declines everything but key
//! decisions, which go to its fallback; so does a reply that names no valid
//! option, a refusal, an HTTP error, or running past `max_calls`. Answers vary
//! between runs (not reproducible); replays reproduce, since choices are
//! logged, and keep the model's reasons as annotations.
//!
//! **It calls a paid API.** Nothing in Kiln's tests or examples calls the
//! real endpoint; tests use a local mock server.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use kiln_engine::{PendingDecision, PlayerView};
use kiln_policy::{Policy, is_key_decision};
use kiln_types::PlayerId;
use serde_json::{Value, json};

use crate::live::{CardText, view_payload};

/// Looks a card's text up by name (owned, for a policy).
pub type SharedOracle = Arc<dyn Fn(&str) -> Option<CardText> + Send + Sync>;

pub const DEFAULT_MODEL: &str = "claude-opus-5-5";
pub const DEFAULT_ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2026-10-21";
/// Re-run a refused request on Anthropic's recommended fallback model.
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmConfig {
    pub model: String,
    /// Ask about every decision (`ask: all`), not just key ones.
    pub ask_all: bool,
    /// At most this many requests per game; then the fallback decides.
    pub max_calls: u32,
    /// `output_config.effort` (`low` … `max`); the model's default if unset.
    pub effort: Option<String>,
    pub endpoint: String,
    /// From `ANTHROPIC_API_KEY` unless set.
    pub api_key: Option<String>,
    pub timeout: Duration,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model: DEFAULT_MODEL.into(),
            ask_all: false,
            max_calls: 200,
            effort: None,
            endpoint: DEFAULT_ENDPOINT.into(),
            api_key: None,
            timeout: Duration::from_secs(300),
        }
    }
}

/// The LLM policy, with a fallback for what it doesn't answer.
pub struct LlmPolicy {
    config: LlmConfig,
    oracle: SharedOracle,
    decks: Vec<String>,
    fallback: Box<dyn Policy>,
    seat: PlayerId,
    calls: u32,
    answered: u64,
    failures: u64,
    /// Events already sent this game.
    shown: usize,
    last_reason: Option<String>,
    errors: Vec<String>,
}

impl std::fmt::Debug for LlmPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmPolicy")
            .field("model", &self.config.model)
            .finish_non_exhaustive()
    }
}

/// What the model is told once per request (it plays one seat).
const SYSTEM: &str = "You are playing one seat in a game of Magic: The Gathering (Commander) run by a rules engine. \
Each request gives you the decision you must make now: its kind and Comprehensive Rules reference, the legal options \
(the engine offers only legal ones), the board as you can see it, recent events, and the Oracle text of the cards involved. \
Rely on the given card text, not memory. Choose the option that best helps you win. Reply with the option's index and a \
one-sentence reason.";

impl LlmPolicy {
    /// `decks` names each seat's deck for the payload.
    pub fn new(
        config: LlmConfig,
        oracle: SharedOracle,
        decks: Vec<String>,
        fallback: Box<dyn Policy>,
    ) -> Self {
        Self {
            config,
            oracle,
            decks,
            fallback,
            seat: PlayerId(0),
            calls: 0,
            answered: 0,
            failures: 0,
            shown: 0,
            last_reason: None,
            errors: Vec::new(),
        }
    }

    fn request_body(&self, payload: &Value) -> Value {
        let mut body = json!({
            "model": self.config.model,
            "max_tokens": 16000,
            "system": SYSTEM,
            "fallbacks": "default",
            "output_config": {
                "format": {
                    "type": "json_schema",
                    "schema": {
                        "type": "object",
                        "properties": {
                            "option": { "type": "integer", "description": "The index of the chosen option" },
                            "reason": { "type": "string" }
                        },
                        "required": ["option", "reason"],
                        "additionalProperties": false
                    }
                }
            },
            "messages": [{
                "role": "user",
                "content": format!(
                    "You are {}. The game, as you see it (JSON):\n{}\n\nChoose one option.",
                    self.seat,
                    serde_json::to_string_pretty(payload).unwrap_or_default()
                )
            }]
        });
        if let Some(e) = &self.config.effort {
            body["output_config"]["effort"] = json!(e);
        }
        body
    }

    /// One request; the chosen index and reason, or why there is none.
    fn ask(&self, payload: &Value, options: usize) -> Result<(u32, String), String> {
        let key = self
            .config
            .api_key
            .clone()
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
            .ok_or_else(|| "ANTHROPIC_API_KEY isn't set".to_owned())?;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(self.config.timeout))
            .http_status_as_error(false)
            .build()
            .into();
        let body = serde_json::to_string(&self.request_body(payload)).map_err(|e| e.to_string())?;
        let mut resp = agent
            .post(&self.config.endpoint)
            .header("content-type", "application/json")
            .header("x-api-key", &key)
            .header("anthropic-version", API_VERSION)
            .header("anthropic-beta", FALLBACK_BETA)
            .send(body)
            .map_err(|e| format!("request failed: {e}"))?;
        let status = resp.status().as_u16();
        let text = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("reading the reply: {e}"))?;
        let v: Value = serde_json::from_str(&text).map_err(|e| format!("reply isn't JSON: {e}"))?;
        if status != 200 {
            let msg = v["error"]["message"].as_str().unwrap_or(&text);
            return Err(format!("HTTP {status}: {msg}"));
        }
        if v["stop_reason"] == "refusal" {
            return Err(format!(
                "the model declined ({})",
                v["stop_details"]["category"]
                    .as_str()
                    .unwrap_or("no category")
            ));
        }
        let answer = v["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<String>();
        let a: Value = serde_json::from_str(answer.trim())
            .map_err(|e| format!("answer isn't JSON ({e}): {answer}"))?;
        let option = a["option"]
            .as_u64()
            .filter(|o| (*o as usize) < options)
            .ok_or_else(|| format!("no valid option in {a}"))?;
        Ok((
            u32::try_from(option).unwrap_or(0),
            a["reason"].as_str().unwrap_or("").to_owned(),
        ))
    }
}

impl Policy for LlmPolicy {
    fn name(&self) -> String {
        format!(
            "llm({}, ask {}) → {}",
            self.config.model,
            if self.config.ask_all { "all" } else { "key" },
            self.fallback.name()
        )
    }

    fn begin_game(&mut self, seat: PlayerId, seed: u64) {
        self.seat = seat;
        self.calls = 0;
        self.answered = 0;
        self.failures = 0;
        self.shown = 0;
        self.last_reason = None;
        self.fallback.begin_game(seat, seed);
    }

    fn try_decide(&mut self, view: &PlayerView<'_>, d: &PendingDecision) -> Option<u32> {
        self.last_reason = None;
        if d.option_count() < 2
            || !(self.config.ask_all || is_key_decision(d))
            || self.calls >= self.config.max_calls
        {
            return None;
        }
        self.calls += 1;
        let oracle = Arc::clone(&self.oracle);
        let payload = view_payload(view, &self.decks, &|n| oracle(n), false, self.shown);
        self.shown = view.event_count();
        match self.ask(&payload, d.option_count()) {
            Ok((option, reason)) => {
                self.answered += 1;
                self.last_reason = Some(format!("{}: {reason}", self.config.model));
                Some(option)
            }
            Err(e) => {
                self.failures += 1;
                if self.errors.len() < 5 {
                    self.errors.push(e);
                }
                None
            }
        }
    }

    fn decide(&mut self, view: &PlayerView<'_>, d: &PendingDecision) -> u32 {
        match self.try_decide(view, d) {
            Some(c) => c,
            None => self.fallback.decide(view, d),
        }
    }

    fn stats(&self) -> BTreeMap<String, u64> {
        let mut m = self.fallback.stats();
        m.insert("llm:answered".into(), self.answered);
        m.insert("llm:failed".into(), self.failures);
        m
    }

    fn take_errors(&mut self) -> Vec<String> {
        let mut e = std::mem::take(&mut self.errors);
        e.extend(self.fallback.take_errors());
        e
    }

    fn rationale(&self) -> Option<String> {
        self.last_reason
            .clone()
            .or_else(|| self.fallback.rationale())
    }
}
