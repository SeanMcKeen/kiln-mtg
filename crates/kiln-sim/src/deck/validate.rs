//! Name resolution and deck-construction validation.
//!
//! M1 supports the Commander format (CR 903.3, 903.5, 702.124). Every
//! violation names the rule it breaks. Banlists come from the card DB's
//! legality data, never from code.

use std::collections::BTreeMap;

use kiln_cards::CoverageLevel;
use kiln_data::names::NameMatch;
use kiln_data::{CardDb, CardRecord, Legality};
use kiln_types::{CardType, Colors, CrRef, OracleId, Supertype};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::parse::{ParsedDeck, Section};

/// Validation settings.
#[derive(Debug, Clone)]
pub struct ValidateOptions {
    /// Scryfall format key. Only `commander` is supported in M1.
    pub format: String,
    /// Commanders named explicitly (e.g. `--commander`), overriding sections and markers.
    pub commanders: Vec<String>,
}

impl Default for ValidateOptions {
    fn default() -> Self {
        Self {
            format: "commander".into(),
            commanders: Vec::new(),
        }
    }
}

/// A violation or warning. `rule` cites the CR when one applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Issue {
    pub code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<CrRef>,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cards: Vec<String>,
}

impl Issue {
    fn new(
        code: &'static str,
        rule: Option<&str>,
        message: impl Into<String>,
        cards: Vec<String>,
    ) -> Self {
        Self {
            code,
            rule: rule.and_then(|r| r.parse().ok()),
            message: message.into(),
            cards,
        }
    }
}

/// Canonical deck identity (`docs/13 Simulation/Deck Import.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeckRecord {
    /// First 12 hex digits of `hash`.
    pub id: String,
    pub name: Option<String>,
    pub commanders: Vec<OracleId>,
    /// Non-commander cards, sorted by Oracle ID.
    pub cards: Vec<(OracleId, u32)>,
    /// SHA-256 over the sorted commander and card lines.
    pub hash: String,
}

/// One resolved card in the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CardLine {
    pub name: String,
    pub oracle_id: OracleId,
    pub qty: u32,
    pub commander: bool,
    pub coverage: CoverageLevel,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub game_changer: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CoverageSummary {
    pub verified: u32,
    pub parsed: u32,
    pub unsupported: u32,
    /// Distinct unsupported reasons and how many cards (by count) carry each.
    pub unsupported_reasons: BTreeMap<String, u32>,
    /// True when every card is verified, i.e. experiments may run without
    /// `--allow-unverified` (FR-S8).
    pub experiment_ready: bool,
}

/// The result of validating a deck.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeckReport {
    pub format: String,
    pub card_db: String,
    pub legal: bool,
    pub deck: Option<DeckRecord>,
    /// Total cards counted toward the deck size, commanders included.
    pub size: u32,
    pub commanders: Vec<String>,
    /// Combined commander color identity (CR 903.4, 702.124c).
    pub color_identity: Option<String>,
    pub violations: Vec<Issue>,
    pub warnings: Vec<Issue>,
    pub coverage: CoverageSummary,
    pub cards: Vec<CardLine>,
}

struct Resolved<'a> {
    card: &'a CardRecord,
    qty: u32,
    commander: bool,
}

/// Resolves and validates a parsed deck.
pub fn validate(db: &CardDb, deck: &ParsedDeck, opts: &ValidateOptions) -> DeckReport {
    validate_with(db, deck, opts, &|c| db.coverage(c))
}

/// [`validate`], with per-card coverage from `coverage` (e.g. the card library).
pub fn validate_with(
    db: &CardDb,
    deck: &ParsedDeck,
    opts: &ValidateOptions,
    coverage_of: &dyn Fn(&CardRecord) -> kiln_cards::Coverage,
) -> DeckReport {
    let mut violations = Vec::new();
    let mut warnings: Vec<Issue> = deck
        .warnings
        .iter()
        .map(|w| Issue::new("parse_warning", None, w.clone(), vec![]))
        .collect();
    let format = opts.format.as_str();
    if format != "commander" {
        violations.push(Issue::new(
            "unsupported_format",
            None,
            format!("format {format:?} isn't supported yet; M1 validates `commander` only"),
            vec![],
        ));
        return report(
            db,
            format,
            deck,
            &[],
            violations,
            warnings,
            None,
            coverage_of,
        );
    }

    // CR 903.5e: no sideboards. Companions and maybeboards are outside the deck too.
    for (section, what) in [
        (Section::Sideboard, "sideboard"),
        (Section::Maybeboard, "maybeboard"),
        (Section::Companion, "companion"),
    ] {
        let n: u32 = deck
            .entries
            .iter()
            .filter(|e| e.section == section)
            .map(|e| e.qty)
            .sum();
        if n > 0 {
            let rule = (section == Section::Sideboard).then_some("903.5e");
            warnings.push(Issue::new(
                "ignored_section",
                rule,
                format!("{n} {what} card(s) ignored; Commander decks don't use sideboards"),
                vec![],
            ));
        }
    }

    // Resolve names.
    let mut resolved: Vec<Resolved<'_>> = Vec::new();
    let explicit: Vec<String> = opts
        .commanders
        .iter()
        .map(|c| c.trim().to_owned())
        .filter(|c| !c.is_empty())
        .collect();
    let mut explicit_found = vec![false; explicit.len()];
    for e in deck
        .entries
        .iter()
        .filter(|e| matches!(e.section, Section::Main | Section::Commander))
    {
        let Some(card) = resolve_one(db, &e.name, format, e.line, &mut violations, &mut warnings)
        else {
            continue;
        };
        let mut commander =
            explicit.is_empty() && (e.section == Section::Commander || e.commander_marker);
        if let Some(pos) = explicit
            .iter()
            .position(|c| resolves_to(db, c, format, card))
        {
            explicit_found[pos] = true;
            commander = true;
        }
        if commander && e.qty > 1 {
            resolved.push(Resolved {
                card,
                qty: 1,
                commander: true,
            });
            resolved.push(Resolved {
                card,
                qty: e.qty - 1,
                commander: false,
            });
        } else {
            resolved.push(Resolved {
                card,
                qty: e.qty,
                commander,
            });
        }
    }
    // Explicit commanders missing from the list are added (exports often omit them).
    for (name, found) in explicit.iter().zip(&explicit_found) {
        if !found
            && let Some(card) = resolve_one(db, name, format, 0, &mut violations, &mut warnings)
        {
            resolved.push(Resolved {
                card,
                qty: 1,
                commander: true,
            });
        }
    }
    if violations
        .iter()
        .any(|v| v.code == "unknown_card" || v.code == "ambiguous_card")
    {
        // Keep checking the rest so the report lists every problem at once.
        warnings.push(Issue::new(
            "partial_validation",
            None,
            "some names didn't resolve; size and singleton checks count only resolved cards",
            vec![],
        ));
    }

    let commanders: Vec<&CardRecord> = resolved
        .iter()
        .filter(|r| r.commander)
        .map(|r| r.card)
        .collect();
    let identity = check_commanders(&commanders, &mut violations);
    check_deck(&resolved, identity, &mut violations);
    report(
        db,
        format,
        deck,
        &resolved,
        violations,
        warnings,
        identity,
        coverage_of,
    )
}

fn resolves_to(db: &CardDb, name: &str, format: &str, card: &CardRecord) -> bool {
    match db.resolve(name, format) {
        NameMatch::Exact(i) | NameMatch::Loose(i) => db.cards()[i].oracle_id == card.oracle_id,
        _ => false,
    }
}

fn resolve_one<'a>(
    db: &'a CardDb,
    name: &str,
    format: &str,
    line: usize,
    violations: &mut Vec<Issue>,
    warnings: &mut Vec<Issue>,
) -> Option<&'a CardRecord> {
    let at = if line > 0 {
        format!("line {line}: ")
    } else {
        String::new()
    };
    match db.resolve(name, format) {
        NameMatch::Exact(i) => Some(&db.cards()[i]),
        NameMatch::Loose(i) => {
            let card = &db.cards()[i];
            warnings.push(Issue::new(
                "loose_name_match",
                None,
                format!("{at}{name:?} matched {:?} ignoring punctuation", card.name),
                vec![card.name.clone()],
            ));
            Some(card)
        }
        NameMatch::Ambiguous(hits) => {
            let names: Vec<String> = hits
                .iter()
                .map(|&i| format!("{} ({})", db.cards()[i].name, db.cards()[i].oracle_id))
                .collect();
            violations.push(Issue::new(
                "ambiguous_card",
                None,
                format!("{at}{name:?} matches several cards; use a more specific name"),
                names,
            ));
            None
        }
        NameMatch::NotFound { suggestions } => {
            let hint = if suggestions.is_empty() {
                String::new()
            } else {
                format!(
                    "; did you mean {}?",
                    suggestions
                        .iter()
                        .map(|s| format!("{s:?}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            violations.push(Issue::new(
                "unknown_card",
                None,
                format!("{at}no card named {name:?}{hint}"),
                suggestions,
            ));
            None
        }
    }
}

/// Partner abilities (CR 702.124a): each pairs differently and they can't be combined (CR 702.124f).
#[derive(Debug, Clone, PartialEq, Eq)]
enum PartnerAbility {
    /// CR 702.124h
    Partner,
    /// CR 702.124i, e.g. "Friends forever"
    PartnerText(String),
    /// CR 702.124j
    PartnerWith(String),
    /// CR 702.124k
    ChooseABackground,
    /// CR 702.124m
    DoctorsCompanion,
}

fn partner_abilities(card: &CardRecord) -> Vec<PartnerAbility> {
    let mut out = Vec::new();
    for line in card.front().reminder_stripped.lines() {
        // The partner's name may itself contain a comma, so take the whole line.
        if let Some(n) = line.trim().strip_prefix("Partner with ") {
            out.push(PartnerAbility::PartnerWith(
                n.trim().trim_end_matches('.').to_owned(),
            ));
            continue;
        }
        for part in line.split(", ") {
            let p = part.trim().trim_end_matches('.');
            if p == "Partner" {
                out.push(PartnerAbility::Partner);
            } else if let Some(t) = p.strip_prefix("Partner—") {
                out.push(PartnerAbility::PartnerText(t.trim().to_owned()));
            } else if let Some(n) = p.strip_prefix("Partner with ") {
                out.push(PartnerAbility::PartnerWith(n.trim().to_owned()));
            } else if p.eq_ignore_ascii_case("Choose a Background") {
                out.push(PartnerAbility::ChooseABackground);
            } else if p == "Doctor's companion" {
                out.push(PartnerAbility::DoctorsCompanion);
            }
        }
    }
    out
}

fn is_background(card: &CardRecord) -> bool {
    let t = &card.front().type_line;
    t.has_supertype(Supertype::Legendary)
        && t.has_type(CardType::Enchantment)
        && t.has_subtype("Background")
}

/// CR 903.3, 903.3a: a legendary creature, Vehicle, or Spacecraft with P/T,
/// or a card that says it can be your commander.
fn commander_eligible(card: &CardRecord) -> bool {
    let f = card.front();
    let t = &f.type_line;
    let text_says_so = card
        .faces
        .iter()
        .any(|face| face.self_ref_text.contains("can be your commander"));
    let legendary = t.has_supertype(Supertype::Legendary);
    let shape = t.has_type(CardType::Creature)
        || t.has_subtype("Vehicle")
        || (t.has_subtype("Spacecraft") && f.power.is_some() && f.toughness.is_some());
    (legendary && shape) || text_says_so
}

/// Whether two commanders may be designated together (CR 702.124).
fn valid_pair(a: &CardRecord, b: &CardRecord) -> bool {
    let (pa, pb) = (partner_abilities(a), partner_abilities(b));
    let doctor = |c: &CardRecord| {
        let t = &c.front().type_line;
        t.has_supertype(Supertype::Legendary)
            && t.has_type(CardType::Creature)
            && t.subtypes.len() == 2
            && t.has_subtype("Time Lord")
            && t.has_subtype("Doctor")
    };
    let one_way = |x: &CardRecord, px: &[PartnerAbility], y: &CardRecord, py: &[PartnerAbility]| {
        px.iter().any(|ab| match ab {
            PartnerAbility::Partner => py.contains(&PartnerAbility::Partner),
            PartnerAbility::PartnerText(t) => py.contains(&PartnerAbility::PartnerText(t.clone())),
            PartnerAbility::PartnerWith(n) => {
                n == &y.name && py.contains(&PartnerAbility::PartnerWith(x.name.clone()))
            }
            PartnerAbility::ChooseABackground => is_background(y),
            PartnerAbility::DoctorsCompanion => doctor(y),
        })
    };
    one_way(a, &pa, b, &pb) || one_way(b, &pb, a, &pa)
}

/// Checks commander designation and returns the combined color identity.
fn check_commanders(commanders: &[&CardRecord], violations: &mut Vec<Issue>) -> Option<Colors> {
    let names = |cs: &[&CardRecord]| cs.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
    match commanders {
        [] => {
            violations.push(Issue::new(
                "no_commander",
                Some("903.3"),
                "no commander designated; put it under a `Commander` header, mark it `*CMDR*`, or pass --commander NAME",
                vec![],
            ));
            return None;
        }
        [a, b] => {
            if !valid_pair(a, b) {
                violations.push(Issue::new(
                    "invalid_partner_pair",
                    Some("702.124f"),
                    format!("{} and {} can't be commanders together: they don't share a partner ability (partner, partner—[text], partner with, choose a Background, Doctor's companion)", a.name, b.name),
                    names(commanders),
                ));
            }
        }
        [_] => {}
        _ => violations.push(Issue::new(
            "too_many_commanders",
            Some("702.124g"),
            format!(
                "{} commanders designated; at most two are allowed",
                commanders.len()
            ),
            names(commanders),
        )),
    }
    for c in commanders {
        // A Background is only a legal commander alongside "choose a Background" (CR 702.124k).
        let paired_background = is_background(c)
            && commanders
                .iter()
                .any(|o| partner_abilities(o).contains(&PartnerAbility::ChooseABackground));
        if !commander_eligible(c) && !paired_background {
            violations.push(Issue::new(
                "ineligible_commander",
                Some("903.3"),
                format!("{} can't be a commander: it must be a legendary creature, Vehicle, or Spacecraft with power/toughness, or say it can be your commander", c.name),
                vec![c.name.clone()],
            ));
        }
    }
    Some(commanders.iter().fold(Colors::COLORLESS, |acc, c| {
        acc.union(c.deck_color_identity())
    }))
}

/// "A deck can have any number of cards named ~" / "up to seven cards named ~".
fn copy_limit(card: &CardRecord) -> Option<u32> {
    let text = card.all_self_ref_text();
    if text.contains("A deck can have any number of cards named ~") {
        return Some(u32::MAX);
    }
    let rest = text.split("A deck can have up to ").nth(1)?;
    let word = rest.split_whitespace().next()?;
    let n = match word {
        "two" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        other => other.parse().ok()?,
    };
    Some(n)
}

fn check_deck(resolved: &[Resolved<'_>], identity: Option<Colors>, violations: &mut Vec<Issue>) {
    // CR 903.5a (and 702.124b with two commanders): exactly 100 cards.
    let size: u32 = resolved.iter().map(|r| r.qty).sum();
    if size != 100 {
        violations.push(Issue::new(
            "deck_size",
            Some("903.5a"),
            format!("deck has {size} cards including commanders; Commander decks have exactly 100"),
            vec![],
        ));
    }

    // Group by card (Oracle ID == one English name; CR 201.3 interchangeable names share it).
    let mut counts: BTreeMap<OracleId, (u32, &CardRecord)> = BTreeMap::new();
    for r in resolved {
        counts.entry(r.card.oracle_id).or_insert((0, r.card)).0 += r.qty;
    }

    let mut singleton = Vec::new();
    let mut banned = Vec::new();
    let mut not_legal = Vec::new();
    let mut off_identity = Vec::new();
    let mut basic_type_off = Vec::new();
    for (count, card) in counts.values() {
        let basic = card.front().type_line.has_supertype(Supertype::Basic);
        let limit = if basic {
            u32::MAX
        } else {
            copy_limit(card).unwrap_or(1)
        };
        if *count > limit {
            singleton.push(format!("{} ×{count}", card.name));
        }
        match card.legality("commander") {
            Legality::Banned => banned.push(card.name.clone()),
            Legality::NotLegal => not_legal.push(card.name.clone()),
            Legality::Legal | Legality::Restricted => {}
        }
        if let Some(id) = identity {
            if !card.deck_color_identity().is_subset_of(id) {
                off_identity.push(format!("{} ({})", card.name, card.deck_color_identity()));
            }
            let produced: Colors = card
                .faces
                .iter()
                .flat_map(|f| f.type_line.basic_land_types().map(|(_, c)| c))
                .collect();
            if !produced.is_subset_of(id) {
                basic_type_off.push(format!(
                    "{} (basic land types produce {})",
                    card.name, produced
                ));
            }
        }
    }
    let mut push = |mut cards: Vec<String>, code: &'static str, rule: Option<&str>, msg: String| {
        cards.sort();
        if !cards.is_empty() {
            violations.push(Issue::new(code, rule, msg, cards));
        }
    };
    let n = singleton.len();
    push(
        singleton,
        "singleton",
        Some("903.5b"),
        format!(
            "{n} card(s) appear more than once; other than basic lands, each card must have a different name"
        ),
    );
    // Banned lists aren't part of the CR; they come from the card DB's legality data.
    let n = banned.len();
    push(
        banned,
        "banned",
        None,
        format!("{n} card(s) are on the Commander banned list (per the card DB's legality data)"),
    );
    let n = not_legal.len();
    push(
        not_legal,
        "not_legal",
        None,
        format!(
            "{n} card(s) aren't legal in Commander per the card DB (e.g. un-cards, digital-only)"
        ),
    );
    if let Some(id) = identity {
        let label = if id.is_colorless() {
            "colorless".to_owned()
        } else {
            id.to_string()
        };
        let n = off_identity.len();
        push(
            off_identity,
            "color_identity",
            Some("903.5c"),
            format!("{n} card(s) are outside the commander color identity {label}"),
        );
        let n = basic_type_off.len();
        push(
            basic_type_off,
            "basic_land_type",
            Some("903.5d"),
            format!("{n} card(s) have basic land types that produce mana outside {label}"),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    db: &CardDb,
    format: &str,
    deck: &ParsedDeck,
    resolved: &[Resolved<'_>],
    violations: Vec<Issue>,
    warnings: Vec<Issue>,
    identity: Option<Colors>,
    coverage_of: &dyn Fn(&CardRecord) -> kiln_cards::Coverage,
) -> DeckReport {
    let mut coverage = CoverageSummary::default();
    let mut cards: Vec<CardLine> = Vec::new();
    for r in resolved {
        let cov = coverage_of(r.card);
        match cov.level() {
            CoverageLevel::Verified => coverage.verified += r.qty,
            CoverageLevel::Parsed => coverage.parsed += r.qty,
            CoverageLevel::Unsupported => coverage.unsupported += r.qty,
        }
        if let kiln_cards::Coverage::Unsupported { reasons } = &cov {
            for reason in reasons {
                *coverage
                    .unsupported_reasons
                    .entry(reason.reason.clone())
                    .or_default() += r.qty;
            }
        }
        match cards
            .iter_mut()
            .find(|c| c.oracle_id == r.card.oracle_id && c.commander == r.commander)
        {
            Some(line) => line.qty += r.qty,
            None => cards.push(CardLine {
                name: r.card.name.clone(),
                oracle_id: r.card.oracle_id,
                qty: r.qty,
                commander: r.commander,
                coverage: cov.level(),
                game_changer: r.card.game_changer,
            }),
        }
    }
    coverage.experiment_ready =
        !resolved.is_empty() && coverage.parsed == 0 && coverage.unsupported == 0;
    cards.sort_by(|a, b| b.commander.cmp(&a.commander).then(a.name.cmp(&b.name)));

    let commanders: Vec<String> = cards
        .iter()
        .filter(|c| c.commander)
        .map(|c| c.name.clone())
        .collect();
    let deck_record = (!resolved.is_empty()).then(|| deck_record(deck.name.clone(), resolved));
    DeckReport {
        format: format.to_owned(),
        card_db: db.id(),
        legal: violations.is_empty(),
        deck: deck_record,
        size: resolved.iter().map(|r| r.qty).sum(),
        commanders,
        color_identity: identity.map(|c| {
            if c.is_colorless() {
                "C".to_owned()
            } else {
                c.to_string()
            }
        }),
        violations,
        warnings,
        coverage,
        cards,
    }
}

fn deck_record(name: Option<String>, resolved: &[Resolved<'_>]) -> DeckRecord {
    let mut commanders: Vec<OracleId> = resolved
        .iter()
        .filter(|r| r.commander)
        .map(|r| r.card.oracle_id)
        .collect();
    commanders.sort();
    let mut counts: BTreeMap<OracleId, u32> = BTreeMap::new();
    for r in resolved.iter().filter(|r| !r.commander) {
        *counts.entry(r.card.oracle_id).or_default() += r.qty;
    }
    let cards: Vec<(OracleId, u32)> = counts.into_iter().collect();
    let mut h = Sha256::new();
    for c in &commanders {
        h.update(format!("C {c}\n"));
    }
    for (id, n) in &cards {
        h.update(format!("{n} {id}\n"));
    }
    let hash: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    DeckRecord {
        id: hash[..12].to_owned(),
        name,
        commanders,
        cards,
        hash,
    }
}
