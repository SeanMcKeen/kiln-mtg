//! `kiln dev card-spike`: measurements for the M3 card-definition spike
//! (`docs/05 Card Definitions/Card Definition Spike Plan.md`, steps A3–A5).
//! Single-threaded on purpose: it is cheap and runs on a laptop.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use kiln_cards::parse::{LineIr, normalize_line, parse_line};
use kiln_cards::render::{matches, render_abilities, render_line};
use kiln_data::{CardDb, CardRecord, Legality};
use kiln_engine::rng::GameRng;
use kiln_types::CardType;
use serde::Serialize;

#[derive(Debug, Default, Serialize)]
pub struct SetStats {
    pub cards: usize,
    /// Every line of every face parsed.
    pub cards_fully_parsed: usize,
    pub lines: usize,
    pub lines_parsed: usize,
    /// Parsed lines whose rendering matches the Oracle line.
    pub lines_round_trip: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct LintStats {
    pub mutants: usize,
    pub caught: usize,
    pub by_kind: BTreeMap<String, (usize, usize)>,
}

#[derive(Debug, Serialize)]
pub struct DefCheck {
    pub card: String,
    pub matches_oracle: bool,
    pub rendered: String,
}

#[derive(Debug, Serialize)]
pub struct SpikeReport {
    pub card_db: String,
    pub curated_pool: SetStats,
    pub sample: SetStats,
    pub all_commander_legal: SetStats,
    pub lint: LintStats,
    pub hand_authored: Vec<DefCheck>,
    /// Most common unparsed line shapes (numbers and mana as placeholders) in the full corpus.
    pub top_unparsed: Vec<(String, usize)>,
    /// phase.rs "supported" on the same sets, when its coverage file was given.
    pub phase_rs: Option<BTreeMap<String, (usize, usize)>>,
    pub curated_unparsed_cards: Vec<String>,
}

fn is_spell(rec: &CardRecord, face: usize) -> bool {
    let t = &rec.faces[face].type_line;
    t.has_type(CardType::Instant) || t.has_type(CardType::Sorcery)
}

/// Parses one card; returns (fully parsed, lines, parsed lines, round-trip lines, parsed line IRs).
fn parse_card(rec: &CardRecord) -> (bool, usize, usize, usize, Vec<(String, LineIr)>) {
    let (mut lines, mut parsed, mut rt) = (0, 0, 0);
    let mut irs = Vec::new();
    for (i, face) in rec.faces.iter().enumerate() {
        for raw in face.reminder_stripped.lines() {
            let line = normalize_line(raw, &rec.name);
            if line.is_empty() {
                continue;
            }
            lines += 1;
            if let Some(ir) = parse_line(&line, is_spell(rec, i)) {
                parsed += 1;
                if matches(&render_line(&ir), &line) {
                    rt += 1;
                    irs.push((line, ir));
                }
            }
        }
    }
    (lines == parsed, lines, parsed, rt, irs)
}

fn add(stats: &mut SetStats, rec: &CardRecord) -> Vec<(String, LineIr)> {
    let (full, lines, parsed, rt, irs) = parse_card(rec);
    stats.cards += 1;
    stats.cards_fully_parsed += usize::from(full);
    stats.lines += lines;
    stats.lines_parsed += parsed;
    stats.lines_round_trip += rt;
    irs
}

/// Mutations applied to a parsed line's IR (through its JSON form).
fn mutants(ir: &LineIr) -> Vec<(&'static str, LineIr)> {
    let Ok(v) = serde_json::to_value(ir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let kinds: [(&str, Mutation); 4] = [
        ("number", bump_number),
        ("card_type", swap_type),
        ("player", swap_player),
        ("may", drop_may),
    ];
    for (name, f) in kinds {
        let mut m = v.clone();
        if f(&mut m)
            && let Ok(back) = serde_json::from_value::<LineIr>(m)
            && &back != ir
        {
            out.push((name, back));
        }
    }
    out
}

/// A mutation of an IR's JSON form; returns whether it changed anything.
type Mutation = fn(&mut serde_json::Value) -> bool;

fn walk(v: &mut serde_json::Value, f: &mut dyn FnMut(&mut serde_json::Value) -> bool) -> bool {
    if f(v) {
        return true;
    }
    match v {
        serde_json::Value::Array(a) => a.iter_mut().any(|x| walk(x, f)),
        serde_json::Value::Object(o) => o.values_mut().any(|x| walk(x, f)),
        _ => false,
    }
}

fn bump_number(v: &mut serde_json::Value) -> bool {
    walk(v, &mut |x| match x.as_i64() {
        Some(n) if x.is_i64() && n > 0 => {
            *x = serde_json::Value::from(n + 1);
            true
        }
        _ => false,
    })
}

fn swap_type(v: &mut serde_json::Value) -> bool {
    walk(v, &mut |x| match x.as_str() {
        Some("creature") => {
            *x = "artifact".into();
            true
        }
        Some("artifact" | "enchantment" | "land" | "planeswalker" | "instant") => {
            *x = "creature".into();
            true
        }
        _ => false,
    })
}

fn swap_player(v: &mut serde_json::Value) -> bool {
    walk(v, &mut |x| match x.as_str() {
        Some("You") | Some("you") => {
            *x = "opponent".into();
            true
        }
        Some("opponent") => {
            *x = "you".into();
            true
        }
        _ => false,
    })
}

fn drop_may(v: &mut serde_json::Value) -> bool {
    walk(v, &mut |x| {
        let inner = x.as_object().and_then(|o| {
            if o.len() == 1 {
                o.get("May").or_else(|| o.get("may")).cloned()
            } else {
                None
            }
        });
        match inner {
            Some(i) => {
                *x = i;
                true
            }
            None => false,
        }
    })
}

/// Replaces digits and mana symbols with placeholders to group similar lines.
fn shape(line: &str) -> String {
    let mut out = String::new();
    let mut in_brace = false;
    for ch in line.chars() {
        match ch {
            '{' => {
                in_brace = true;
                out.push_str("{M}");
            }
            '}' => in_brace = false,
            _ if in_brace => {}
            c if c.is_ascii_digit() => {
                if !out.ends_with('N') {
                    out.push('N');
                }
            }
            c => out.push(c),
        }
    }
    out.chars().take(80).collect()
}

pub fn run(
    db: &CardDb,
    sample_size: usize,
    seed: u64,
    defs_dir: &Path,
    pool_file: &Path,
    phase_coverage: Option<&Path>,
) -> Result<SpikeReport> {
    let legal: Vec<&CardRecord> = db
        .cards()
        .iter()
        .filter(|c| c.layout.is_deck_card() && c.legality("commander") == Legality::Legal)
        .collect();
    // Seeded sample without replacement (Fisher–Yates prefix over indices).
    let mut idx: Vec<usize> = (0..legal.len()).collect();
    let mut rng = GameRng::new(seed);
    let n = sample_size.min(idx.len());
    for i in 0..n {
        let j = i + rng.index(idx.len() - i);
        idx.swap(i, j);
    }
    let sample: Vec<&CardRecord> = idx[..n].iter().map(|&i| legal[i]).collect();

    let pool_names: Vec<String> = std::fs::read_to_string(pool_file)
        .with_context(|| format!("reading {}", pool_file.display()))?
        .lines()
        .map(str::to_owned)
        .filter(|l| !l.is_empty())
        .collect();
    let resolve = |name: &str| match db.resolve(name, "commander") {
        kiln_data::NameMatch::Exact(i) | kiln_data::NameMatch::Loose(i) => Some(&db.cards()[i]),
        _ => None,
    };
    let pool: Vec<&CardRecord> = pool_names.iter().filter_map(|n| resolve(n)).collect();

    let mut report = SpikeReport {
        card_db: db.id(),
        curated_pool: SetStats::default(),
        sample: SetStats::default(),
        all_commander_legal: SetStats::default(),
        lint: LintStats::default(),
        hand_authored: Vec::new(),
        top_unparsed: Vec::new(),
        phase_rs: None,
        curated_unparsed_cards: Vec::new(),
    };
    for rec in &pool {
        add(&mut report.curated_pool, rec);
        if !parse_card(rec).0 {
            report.curated_unparsed_cards.push(rec.name.clone());
        }
    }
    let mut lint_lines = Vec::new();
    for rec in &sample {
        lint_lines.extend(add(&mut report.sample, rec));
    }
    let mut shapes: BTreeMap<String, usize> = BTreeMap::new();
    for rec in &legal {
        add(&mut report.all_commander_legal, rec);
        for (i, face) in rec.faces.iter().enumerate() {
            for raw in face.reminder_stripped.lines() {
                let line = normalize_line(raw, &rec.name);
                if !line.is_empty() && parse_line(&line, is_spell(rec, i)).is_none() {
                    *shapes.entry(shape(&line)).or_default() += 1;
                }
            }
        }
    }
    let mut top: Vec<(String, usize)> = shapes.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    top.truncate(25);
    report.top_unparsed = top;

    // A4: injected mis-parses on round-tripping sample lines.
    for (line, ir) in &lint_lines {
        for (kind, m) in mutants(ir) {
            let caught = !matches(&render_line(&m), line);
            report.lint.mutants += 1;
            report.lint.caught += usize::from(caught);
            let e = report.lint.by_kind.entry(kind.to_owned()).or_default();
            e.0 += 1;
            e.1 += usize::from(caught);
        }
    }

    // Hand-authored definitions rendered against Oracle text.
    if defs_dir.exists() {
        for def in kiln_cards::defs::load_dir(defs_dir)? {
            let Some(rec) = resolve(&def.card) else {
                continue;
            };
            let rendered: Vec<String> = def
                .faces
                .iter()
                .map(|f| render_abilities(&f.abilities))
                .collect();
            let oracle: Vec<String> = rec
                .faces
                .iter()
                .flat_map(|f| {
                    f.reminder_stripped
                        .lines()
                        .map(|l| normalize_line(l, &rec.name))
                })
                .collect();
            let rendered_text = rendered.join("\n");
            report.hand_authored.push(DefCheck {
                card: def.card.clone(),
                matches_oracle: matches(&rendered_text, &oracle.join("\n")),
                rendered: rendered_text,
            });
        }
    }

    // A5: phase.rs published coverage on the same sets.
    if let Some(path) = phase_coverage {
        let v: serde_json::Value = serde_json::from_slice(
            &std::fs::read(path).with_context(|| format!("reading {}", path.display()))?,
        )?;
        let mut supported: BTreeMap<String, bool> = BTreeMap::new();
        if let Some(cards) = v.get("cards").and_then(|c| c.as_array()) {
            for c in cards {
                if let (Some(n), Some(s)) = (
                    c.get("card_name").and_then(|x| x.as_str()),
                    c.get("supported").and_then(serde_json::Value::as_bool),
                ) {
                    supported.entry(n.to_lowercase()).or_insert(s);
                }
            }
        }
        let count = |set: &[&CardRecord]| {
            let found: Vec<bool> = set
                .iter()
                .filter_map(|r| supported.get(&r.name.to_lowercase()).copied())
                .collect();
            (found.len(), found.iter().filter(|s| **s).count())
        };
        let mut m = BTreeMap::new();
        m.insert("curated_pool (found, supported)".to_owned(), count(&pool));
        m.insert("sample (found, supported)".to_owned(), count(&sample));
        m.insert(
            "all_commander_legal (found, supported)".to_owned(),
            count(&legal),
        );
        report.phase_rs = Some(m);
    }
    Ok(report)
}
