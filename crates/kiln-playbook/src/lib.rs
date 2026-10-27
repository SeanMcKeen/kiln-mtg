//! `kiln-playbook`: sandboxed playbook policies authored by AI agents
//! (`docs/14 Players & Policies/B - Playbooks.md`, M8 plan P1–P3).
//!
//! A playbook is a [Rhai](https://rhai.rs) script. For each decision it may
//! define a function named after the decision kind (`mulligan`, `priority`,
//! `choose_target`, `declare_attacker`, …) or a catch-all `decide`, taking
//! `(view, decision)`:
//! - `view` is the deciding player's redacted view (the same snapshot the
//!   hidden-information auditor checks): `me`, `turn`, `players`, `hand`,
//!   `battlefield`, `stack`, `graveyards`, …
//! - `decision` has `kind`, `options` (labels such as "Cast Sol Ring") and `count`.
//!
//! It returns an option index or an option label to choose, or nothing (`()`)
//! to **defer** to the fallback policy. Errors, operation limits and invalid
//! answers also defer, and are counted: a playbook can't crash a run or make
//! an illegal choice.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use kiln_engine::{DecisionKind, Game, PendingDecision, PlayerView};
use kiln_policy::Policy;
use kiln_types::PlayerId;
use rhai::{AST, Array, Dynamic, Engine, Map, Scope};

/// Operations one call may take (Rhai's sandbox counter).
pub const MAX_OPERATIONS: u64 = 200_000;

/// A compile error, with the position Rhai reports.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{name}:{line}:{column}: {message}")]
pub struct PlaybookError {
    pub name: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
}

std::thread_local! {
    static TAGS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A compiled playbook, shared by every game that uses it.
pub struct Playbook {
    pub name: String,
    engine: Engine,
    ast: AST,
    hooks: BTreeSet<String>,
}

impl std::fmt::Debug for Playbook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Playbook")
            .field("name", &self.name)
            .field("hooks", &self.hooks)
            .finish_non_exhaustive()
    }
}

/// The view as playbooks see it. Fields are built when the script reads
/// them, always through [`Game::view`] (no hidden information): `me`, `turn`,
/// `active`, `my_turn`, `step`, `players`, `hand`, `battlefield`, `stack`,
/// `graveyard` (mine), `graveyards` (every player's), `command`. Objects carry
/// `name`, `types`, `mana_value`, `land`, `creature`, `controller`, `owner`,
/// `tapped`, `power`, `toughness`, `attacking`, `token` and `commander`.
#[derive(Clone)]
struct ScriptView {
    game: Arc<Game>,
    me: PlayerId,
}

impl ScriptView {
    fn view(&self) -> PlayerView<'_> {
        self.game.view(self.me)
    }
}

fn seat(p: PlayerId) -> Dynamic {
    Dynamic::from(i64::from(p.0))
}

fn object(view: &PlayerView<'_>, id: kiln_types::ObjectId) -> Option<Dynamic> {
    let o = view.object(id)?;
    let d = view.definition(o.def);
    let mut m = Map::new();
    m.insert("name".into(), o.name.into());
    m.insert(
        "types".into(),
        d.type_line
            .card_types
            .iter()
            .map(|t| Dynamic::from(t.word().to_lowercase()))
            .collect::<Array>()
            .into(),
    );
    m.insert(
        "mana_value".into(),
        Dynamic::from(i64::from(d.mana_value())),
    );
    m.insert("land".into(), d.is(kiln_types::CardType::Land).into());
    m.insert(
        "creature".into(),
        d.is(kiln_types::CardType::Creature).into(),
    );
    m.insert("controller".into(), seat(o.controller));
    m.insert("owner".into(), seat(o.owner));
    m.insert("tapped".into(), o.tapped.into());
    m.insert(
        "power".into(),
        o.power
            .map_or(Dynamic::UNIT, |x| Dynamic::from(i64::from(x))),
    );
    m.insert(
        "toughness".into(),
        o.toughness
            .map_or(Dynamic::UNIT, |x| Dynamic::from(i64::from(x))),
    );
    m.insert("attacking".into(), o.attacking.is_some().into());
    m.insert("token".into(), o.token.into());
    m.insert("commander".into(), o.commander.into());
    Some(Dynamic::from_map(m))
}

fn objects(view: &PlayerView<'_>, ids: &[kiln_types::ObjectId]) -> Array {
    ids.iter().filter_map(|id| object(view, *id)).collect()
}

fn register_view(e: &mut Engine) {
    e.register_type_with_name::<ScriptView>("View")
        .register_get("me", |v: &mut ScriptView| seat(v.me))
        .register_get("turn", |v: &mut ScriptView| {
            Dynamic::from(i64::from(v.view().turn().number))
        })
        .register_get("active", |v: &mut ScriptView| seat(v.view().turn().active))
        .register_get("my_turn", |v: &mut ScriptView| {
            v.view().turn().active == v.me
        })
        .register_get("step", |v: &mut ScriptView| {
            format!("{:?}", v.view().turn().step)
        })
        .register_get("players", |v: &mut ScriptView| -> Array {
            let view = v.view();
            (0..view.player_count())
                .map(|i| {
                    let p = view.player(PlayerId(u8::try_from(i).unwrap_or(0)));
                    let mut m = Map::new();
                    m.insert("seat".into(), seat(p.id));
                    m.insert("in_game".into(), p.in_game.into());
                    m.insert("life".into(), Dynamic::from(p.life));
                    m.insert("poison".into(), Dynamic::from(i64::from(p.poison)));
                    m.insert(
                        "hand_count".into(),
                        Dynamic::from(i64::try_from(p.hand_count).unwrap_or(0)),
                    );
                    m.insert(
                        "library_count".into(),
                        Dynamic::from(i64::try_from(p.library_count).unwrap_or(0)),
                    );
                    Dynamic::from_map(m)
                })
                .collect()
        })
        .register_get("hand", |v: &mut ScriptView| {
            let view = v.view();
            objects(&view, view.hand())
        })
        .register_get("battlefield", |v: &mut ScriptView| {
            let view = v.view();
            objects(&view, view.battlefield())
        })
        .register_get("graveyard", |v: &mut ScriptView| {
            let view = v.view();
            objects(&view, view.graveyard(v.me))
        })
        .register_get("graveyards", |v: &mut ScriptView| -> Array {
            let view = v.view();
            (0..view.player_count())
                .map(|i| {
                    Dynamic::from_array(objects(
                        &view,
                        view.graveyard(PlayerId(u8::try_from(i).unwrap_or(0))),
                    ))
                })
                .collect()
        })
        .register_get("command", |v: &mut ScriptView| {
            let view = v.view();
            objects(&view, view.command())
        })
        .register_get("stack", |v: &mut ScriptView| -> Array {
            let view = v.view();
            view.stack()
                .into_iter()
                .map(|e| {
                    let d = view.definition(e.def);
                    let mut m = Map::new();
                    m.insert("name".into(), e.name.into());
                    m.insert("controller".into(), seat(e.controller));
                    m.insert("ability".into(), e.ability.is_some().into());
                    m.insert(
                        "mana_value".into(),
                        Dynamic::from(i64::from(d.mana_value())),
                    );
                    m.insert(
                        "creature".into(),
                        d.is(kiln_types::CardType::Creature).into(),
                    );
                    Dynamic::from_map(m)
                })
                .collect()
        });
}

/// The snake-case name of a decision kind ("ChooseTarget" → "choose_target").
pub fn kind_name(k: &DecisionKind) -> String {
    let debug = format!("{k:?}");
    let variant = debug.split([' ', '{', '(']).next().unwrap_or("");
    let mut out = String::new();
    for (i, c) in variant.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn sandbox() -> Engine {
    let mut e = Engine::new();
    e.set_max_operations(MAX_OPERATIONS)
        .set_max_call_levels(32)
        .set_max_expr_depths(64, 32)
        .set_max_string_size(10_000)
        .set_max_array_size(2_000)
        .set_max_map_size(2_000);
    // No output channel: a playbook only answers decisions.
    e.on_print(|_| {});
    e.on_debug(|_, _, _| {});
    e.register_fn("find_option", |options: Array, text: &str| -> i64 {
        let text = text.to_lowercase();
        options
            .iter()
            .position(|o| {
                o.clone()
                    .into_string()
                    .is_ok_and(|s| s.to_lowercase().contains(&text))
            })
            .and_then(|i| i64::try_from(i).ok())
            .unwrap_or(-1)
    });
    e.register_fn("has_option", |options: Array, text: &str| -> bool {
        let text = text.to_lowercase();
        options.iter().any(|o| {
            o.clone()
                .into_string()
                .is_ok_and(|s| s.to_lowercase().contains(&text))
        })
    });
    e.register_fn("names", |cards: Array| -> Array {
        cards
            .into_iter()
            .filter_map(|c| c.try_cast::<Map>().and_then(|m| m.get("name").cloned()))
            .collect()
    });
    e.register_fn("tag", |name: &str| {
        TAGS.with(|t| t.borrow_mut().push(name.to_owned()));
    });
    register_view(&mut e);
    e
}

impl Playbook {
    /// Compiles a playbook. `name` labels errors (a file name).
    pub fn compile(name: &str, text: &str) -> Result<Self, PlaybookError> {
        let engine = sandbox();
        let ast = engine.compile(text).map_err(|e| {
            let pos = e.position();
            PlaybookError {
                name: name.to_owned(),
                line: pos.line().unwrap_or(0),
                column: pos.position().unwrap_or(0),
                message: e.err_type().to_string(),
            }
        })?;
        let hooks = ast
            .iter_functions()
            .filter(|f| f.params.len() == 2)
            .map(|f| f.name.to_owned())
            .collect();
        Ok(Self {
            name: name.to_owned(),
            engine,
            ast,
            hooks,
        })
    }

    /// The functions it defines that answer decisions.
    pub fn hooks(&self) -> &BTreeSet<String> {
        &self.hooks
    }

    /// The function that answers `kind`, if any.
    fn hook(&self, kind: &str) -> Option<&str> {
        self.hooks
            .get(kind)
            .or_else(|| self.hooks.get("decide"))
            .map(String::as_str)
    }

    /// Asks the playbook. `Ok(None)` means it deferred.
    pub fn decide(
        &self,
        view: &PlayerView<'_>,
        d: &PendingDecision,
        tags: &mut Vec<String>,
    ) -> Result<Option<u32>, String> {
        let kind = kind_name(&d.kind);
        let Some(hook) = self.hook(&kind) else {
            return Ok(None);
        };
        let count = d.option_count();
        let labels: Vec<String> = (0..count)
            .map(|i| view.option_label(i).unwrap_or_default())
            .collect();
        let view_value = Dynamic::from(ScriptView {
            game: Arc::new(view.game_copy()),
            me: view.me(),
        });
        let mut decision = Map::new();
        decision.insert("kind".into(), kind.clone().into());
        decision.insert(
            "options".into(),
            labels
                .iter()
                .cloned()
                .map(Dynamic::from)
                .collect::<Array>()
                .into(),
        );
        decision.insert(
            "count".into(),
            Dynamic::from(i64::try_from(count).unwrap_or(0)),
        );
        decision.insert("player".into(), Dynamic::from(i64::from(d.player.0)));
        TAGS.with(|t| t.borrow_mut().clear());
        let result = self
            .engine
            .call_fn::<Dynamic>(
                &mut Scope::new(),
                &self.ast,
                hook,
                (view_value, Dynamic::from_map(decision)),
            )
            .map_err(|e| format!("{hook}: {e}"));
        tags.extend(TAGS.with(|t| std::mem::take(&mut *t.borrow_mut())));
        let answer = result?;
        if answer.is_unit() {
            return Ok(None);
        }
        if let Ok(i) = answer.as_int() {
            return match u32::try_from(i) {
                Ok(i) if (i as usize) < count => Ok(Some(i)),
                _ => Err(format!("{hook} returned option {i}, but there are {count}")),
            };
        }
        if let Ok(label) = answer.into_string() {
            return labels
                .iter()
                .position(|l| *l == label)
                .or_else(|| {
                    let low = label.to_lowercase();
                    labels.iter().position(|l| l.to_lowercase().contains(&low))
                })
                .map(|i| Some(u32::try_from(i).unwrap_or(0)))
                .ok_or_else(|| format!("{hook} returned {label:?}, which isn't an option"));
        }
        Err(format!(
            "{hook} returned something that isn't an option index or label"
        ))
    }
}

/// A playbook with a fallback for the decisions it doesn't answer.
pub struct PlaybookPolicy {
    book: Arc<Playbook>,
    fallback: Box<dyn Policy>,
    stats: BTreeMap<String, u64>,
    /// The first few runtime errors, for validation reports.
    pub errors: Vec<String>,
}

impl std::fmt::Debug for PlaybookPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlaybookPolicy")
            .field("book", &self.book.name)
            .field("fallback", &self.fallback.name())
            .finish_non_exhaustive()
    }
}

impl PlaybookPolicy {
    pub fn new(book: Arc<Playbook>, fallback: Box<dyn Policy>) -> Self {
        Self {
            book,
            fallback,
            stats: BTreeMap::new(),
            errors: Vec::new(),
        }
    }

    fn count(&mut self, key: &str) {
        *self.stats.entry(key.to_owned()).or_default() += 1;
    }
}

impl Policy for PlaybookPolicy {
    fn name(&self) -> String {
        format!("playbook({}) → {}", self.book.name, self.fallback.name())
    }

    fn begin_game(&mut self, seat: PlayerId, seed: u64) {
        self.fallback.begin_game(seat, seed);
        self.stats.clear();
    }

    fn try_decide(&mut self, view: &PlayerView<'_>, d: &PendingDecision) -> Option<u32> {
        let mut tags = Vec::new();
        let answer = self.book.decide(view, d, &mut tags);
        for t in tags {
            self.count(&format!("tag:{t}"));
        }
        match answer {
            Ok(Some(i)) => {
                self.count("playbook:answered");
                Some(i)
            }
            Ok(None) => None,
            Err(e) => {
                self.count("playbook:errors");
                if self.errors.len() < 20 {
                    self.errors.push(e);
                }
                None
            }
        }
    }

    fn decide(&mut self, view: &PlayerView<'_>, d: &PendingDecision) -> u32 {
        if let Some(i) = self.try_decide(view, d) {
            return i;
        }
        self.count("fallback:answered");
        self.fallback.decide(view, d)
    }

    fn stats(&self) -> BTreeMap<String, u64> {
        let mut out = self.stats.clone();
        for (k, v) in self.fallback.stats() {
            *out.entry(k).or_default() += v;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_errors_have_positions() {
        let e = Playbook::compile("bad.rhai", "fn priority(view, d) {\n  let x = ;\n}")
            .expect_err("syntax error");
        assert_eq!(e.line, 2);
        assert!(e.to_string().starts_with("bad.rhai:2:"), "{e}");
    }

    #[test]
    fn hooks_are_two_argument_functions() {
        let b = Playbook::compile(
            "p",
            "fn mulligan(view, d) { 0 }\nfn helper(x) { x }\nfn decide(view, d) { () }",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            b.hooks().iter().cloned().collect::<Vec<_>>(),
            vec!["decide".to_owned(), "mulligan".to_owned()]
        );
        assert_eq!(b.hook("mulligan"), Some("mulligan"));
        assert_eq!(b.hook("priority"), Some("decide"));
    }

    #[test]
    fn kind_names_are_snake_case() {
        assert_eq!(
            kind_name(&DecisionKind::Mulligan { mulligans_taken: 0 }),
            "mulligan"
        );
    }

    #[test]
    fn runaway_scripts_hit_the_operation_limit() {
        let b = Playbook::compile("loop", "fn f() { loop { } }").unwrap_or_else(|e| panic!("{e}"));
        let r = b
            .engine
            .call_fn::<Dynamic>(&mut Scope::new(), &b.ast, "f", ());
        assert!(r.is_err());
    }
}
