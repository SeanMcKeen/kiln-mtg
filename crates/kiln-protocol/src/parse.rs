//! Reading and validating a `kiln-spec/1` document (Experiment Spec,
//! "Validation rules"). Card-level checks (legality, coverage) need the card
//! DB and live in `kiln-sim`.

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

use crate::spec::{Mode, PolicySpec, ReplayKeep, SPEC_VERSION, Seating, Spec};

/// Why a spec was rejected (CLI exit code 2).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SpecError {
    #[error("can't read the spec: {0}")]
    Parse(String),
    #[error("invalid spec:\n  - {}", .0.join("\n  - "))]
    Invalid(Vec<String>),
}

/// A validated spec with defaults filled in, its hash, and warnings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSpec {
    pub spec: Spec,
    /// First 16 hex digits of the SHA-256 of the resolved spec's JSON.
    pub hash: String,
    pub warnings: Vec<String>,
}

/// Parses YAML or JSON. Unknown keys and values are errors that name the
/// closest valid one.
pub fn parse_spec(text: &str) -> Result<Spec, SpecError> {
    // YAML is a superset of JSON. Going through a JSON value lets externally
    // tagged enums (`source: { file: … }`) read naturally.
    let value: serde_json::Value =
        yaml_serde::from_str(text).map_err(|e| SpecError::Parse(e.to_string()))?;
    serde_json::from_value(value).map_err(|e| SpecError::Parse(with_suggestion(&e.to_string())))
}

/// "unknown field `gmaes`, expected one of `games`, …" gains "(did you mean `games`?)".
fn with_suggestion(msg: &str) -> String {
    if !(msg.contains("unknown field") || msg.contains("unknown variant")) {
        return msg.to_owned();
    }
    let names: Vec<&str> = msg.split('`').skip(1).step_by(2).collect();
    let Some((bad, valid)) = names.split_first() else {
        return msg.to_owned();
    };
    let best = valid
        .iter()
        .map(|v| (strsim::jaro_winkler(bad, v), *v))
        .filter(|(score, _)| *score >= 0.7)
        .max_by(|a, b| a.0.total_cmp(&b.0));
    match best {
        Some((_, v)) => format!("{msg} (did you mean `{v}`?)"),
        None => msg.to_owned(),
    }
}

/// How many seat orders `seating` cycles through for `players` seats.
pub fn rotation_count(seating: Seating, players: usize) -> u64 {
    match seating {
        Seating::Fixed | Seating::Random => 1,
        Seating::BalancedRotation => players as u64,
        Seating::AllPermutations => (1..=players as u64).product(),
    }
}

/// Seats in a game of this spec.
pub fn players(spec: &Spec) -> usize {
    match spec.mode {
        Mode::Goldfish => 1 + usize::from(spec.goldfish.clone().unwrap_or_default().opponents),
        Mode::Pod | Mode::Ab | Mode::OpeningHands => spec.decks.len(),
    }
}

/// Checks a parsed spec and fills in defaults that depend on other fields.
pub fn validate(mut spec: Spec) -> Result<ResolvedSpec, SpecError> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    if spec.spec != SPEC_VERSION {
        errors.push(format!(
            "`spec` is {:?}; this version reads {SPEC_VERSION:?}",
            spec.spec
        ));
    }
    if spec.name.is_empty()
        || !spec
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        errors.push(format!(
            "`name` {:?} must be letters, digits, '-', '_' or '.' (it names the run directory)",
            spec.name
        ));
    }
    if spec.format != "commander" {
        errors.push(format!(
            "`format` {:?} isn't supported yet; use commander",
            spec.format
        ));
    }
    if spec.games == 0 {
        errors.push("`games` must be at least 1".into());
    }
    let ids: Vec<&str> = spec.decks.iter().map(|d| d.id.as_str()).collect();
    let mut seen = BTreeSet::new();
    for id in &ids {
        if id.is_empty() {
            errors.push("a deck has an empty `id`".into());
        } else if !seen.insert(*id) {
            errors.push(format!("deck id `{id}` is used twice"));
        }
    }
    let n = players(&spec);
    match spec.mode {
        Mode::Pod | Mode::Ab if !(2..=8).contains(&spec.decks.len()) => {
            errors.push(format!(
                "a {} needs 2 to 8 decks (one per seat); there are {}",
                if spec.mode == Mode::Pod {
                    "pod"
                } else {
                    "A/B run"
                },
                spec.decks.len()
            ));
        }
        Mode::Goldfish if spec.decks.len() != 1 => {
            errors.push(format!(
                "goldfish takes exactly one deck; there are {}",
                spec.decks.len()
            ));
        }
        Mode::Goldfish if !(1..=7).contains(&(n - 1)) => {
            errors.push("`goldfish.opponents` must be 1 to 7".into());
        }
        Mode::OpeningHands if spec.decks.is_empty() => {
            errors.push("opening_hands needs at least one deck".into());
        }
        _ => {}
    }
    if spec.mode != Mode::Goldfish && spec.goldfish.is_some() {
        errors.push("`goldfish` settings need `mode: goldfish`".into());
    }
    if spec.mode != Mode::OpeningHands && spec.opening_hands.is_some() {
        errors.push("`opening_hands` settings need `mode: opening_hands`".into());
    }
    if spec.mode == Mode::Ab {
        if spec.variants.is_empty() {
            errors.push("an A/B run needs at least one entry in `variants`".into());
        }
        for (name, v) in &spec.variants {
            if !ids.contains(&v.base.as_str()) {
                errors.push(format!(
                    "variant `{name}`: base `{}` isn't a deck id",
                    v.base
                ));
            }
            if v.swap.out.len() != v.swap.add.len() {
                errors.push(format!(
                    "variant `{name}`: `swap.out` and `swap.in` must have the same length (the deck stays 100 cards)"
                ));
            }
        }
    } else if !spec.variants.is_empty() {
        errors.push("`variants` need `mode: ab`".into());
    }
    for (key, policy) in &spec.policies {
        if key != "default" && !ids.contains(&key.as_str()) {
            errors.push(format!("`policies.{key}` isn't a deck id (or `default`)"));
        }
        if let Some(m) = unavailable(policy) {
            errors.push(format!(
                "`policies.{key}`: kind `{}` arrives in {m}; use random, greedy or passive",
                policy.kind()
            ));
        }
    }
    for d in &spec.metrics.per_card.decks {
        if !ids.contains(&d.as_str()) {
            errors.push(format!("`metrics.per_card.decks`: `{d}` isn't a deck id"));
        }
    }
    if ReplayKeep::parse(&spec.outputs.replays.keep).is_none() {
        errors.push(format!(
            "`outputs.replays.keep` {:?} must be none, all, interesting or sample:N",
            spec.outputs.replays.keep
        ));
    }
    if spec.limits.max_turns == 0 || spec.limits.max_decisions == 0 {
        errors.push("`limits.max_turns` and `limits.max_decisions` must be at least 1".into());
    }
    if !errors.is_empty() {
        return Err(SpecError::Invalid(errors));
    }

    // Balanced seating needs whole cycles of seat orders.
    if spec.mode != Mode::OpeningHands {
        let k = rotation_count(spec.seating, n);
        if !spec.games.is_multiple_of(k) {
            let rounded = spec.games.div_ceil(k) * k;
            warnings.push(format!(
                "games rounded up from {} to {rounded}: {:?} seating cycles through {k} seat orders",
                spec.games, spec.seating
            ));
            spec.games = rounded;
        }
    }
    if spec.limits.max_wall_ms.is_some() {
        warnings.push(
            "limits.max_wall_ms is set: which games are capped can depend on the machine, so results may not reproduce exactly"
                .into(),
        );
    }
    if spec.outputs.dir.is_none() {
        spec.outputs.dir = Some(format!("runs/{}", spec.name));
    }
    let hash = spec_hash(&spec);
    Ok(ResolvedSpec {
        spec,
        hash,
        warnings,
    })
}

/// Parses and validates.
pub fn load_spec(text: &str) -> Result<ResolvedSpec, SpecError> {
    validate(parse_spec(text)?)
}

/// The milestone that brings a policy kind, if it isn't available yet.
fn unavailable(p: &PolicySpec) -> Option<&'static str> {
    match p {
        PolicySpec::Random { .. } | PolicySpec::Greedy | PolicySpec::Passive => None,
        PolicySpec::Playbook { .. } => Some("M8"),
        PolicySpec::Search { .. } => Some("M9"),
        PolicySpec::Llm { .. } => Some("M10"),
    }
}

/// SHA-256 of the spec's JSON (struct fields in declaration order, maps sorted).
pub fn spec_hash(spec: &Spec) -> String {
    let json = serde_json::to_vec(spec).unwrap_or_default();
    let digest = Sha256::digest(&json);
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const POD: &str = r#"
spec: kiln-spec/1
name: test-pod
games: 10
decks:
  - { id: a, source: { file: a.txt } }
  - { id: b, source: { text: "1 Sol Ring" } }
  - { id: c, source: { file: c.txt } }
  - { id: d, source: { moxfield_export: "1 Island" } }
policies:
  default: { kind: random, pass: 300 }
  a: { kind: greedy }
"#;

    #[test]
    fn pod_defaults_and_rounding() {
        let r = load_spec(POD).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            r.spec.games, 12,
            "10 rounded up to a multiple of 4 rotations"
        );
        assert_eq!(r.warnings.len(), 1);
        assert_eq!(r.spec.outputs.dir.as_deref(), Some("runs/test-pod"));
        assert_eq!(r.spec.limits.max_turns, 100);
        assert!(r.spec.metrics.win_rates && !r.spec.metrics.per_card.enabled);
        assert_eq!(r.spec.policies["default"], PolicySpec::Random { pass: 300 });
        assert_eq!(r.hash.len(), 16);
        assert_eq!(
            r.hash,
            load_spec(POD).unwrap_or_else(|e| panic!("{e}")).hash
        );
    }

    #[test]
    fn unknown_keys_get_suggestions() {
        let e = parse_spec("spec: kiln-spec/1\nname: x\ngmaes: 5\n")
            .expect_err("an unknown key is an error");
        assert!(e.to_string().contains("did you mean `games`?"), "{e}");
        let e = parse_spec("spec: kiln-spec/1\nname: x\nseating: balanced_rotaton\n")
            .expect_err("an unknown key is an error");
        assert!(
            e.to_string().contains("did you mean `balanced_rotation`?"),
            "{e}"
        );
    }

    #[test]
    fn per_card_accepts_a_flag_or_a_map() {
        let s = parse_spec("spec: kiln-spec/1\nname: x\nmetrics: { per_card: true }\n")
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(s.metrics.per_card.enabled);
        let s = parse_spec(
            "spec: kiln-spec/1\nname: x\nmetrics: { per_card: { decks: [a], min_samples: 5 } }\n",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert!(s.metrics.per_card.enabled);
        assert_eq!(s.metrics.per_card.min_samples, 5);
    }

    #[test]
    fn invalid_specs_list_every_problem() {
        let text = POD.replace("a: { kind: greedy }", "zz: { kind: search }");
        let Err(SpecError::Invalid(errors)) = load_spec(&text) else {
            panic!("expected errors")
        };
        assert!(
            errors
                .iter()
                .any(|e| e.contains("`policies.zz` isn't a deck id"))
        );
        assert!(errors.iter().any(|e| e.contains("arrives in M9")));
    }

    #[test]
    fn goldfish_needs_one_deck() {
        let r = load_spec(
            "spec: kiln-spec/1\nname: g\nmode: goldfish\ngames: 8\ndecks: [{ id: k, source: { file: k.txt } }]\ngoldfish: { opponents: 3 }\n",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(players(&r.spec), 4);
        assert_eq!(r.spec.games, 8);
    }
}
