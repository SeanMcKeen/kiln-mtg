//! IR → canonical English renderer and lint (M3 spike, step A4).
//!
//! Renders IR back into templated Oracle-style text. Comparing the rendering
//! with the Oracle line catches mis-parses and mis-authored definitions: a
//! wrong number, type, player or target shows up as a text difference.
//! Comparison uses [`canonical`] tokens so harmless phrasing differences
//! ("and" vs a new sentence) don't count.

use crate::ir::{
    Ability, CostPart, Destination, DiscardSpec, Effect, Keyword, ManaSpec, ObjectFilter,
    ObjectRef, ObjectSet, PlayerRef, PlayerRel, Quantity, QuantityExpr, Recipient, ReplaceEvent,
    StaticEffect, StepName, TargetKind, TargetSpec, Timing, TokenSpec, Trigger,
};
use crate::parse::LineIr;

/// Canonical tokens for comparison: lowercase, punctuation removed, number
/// words as digits, and the connectives "and", "then", "you" dropped;
/// "whenever" is folded into "when".
pub fn canonical(text: &str) -> Vec<String> {
    let cleaned: String = text
        .to_lowercase()
        .chars()
        .map(|c| {
            if matches!(c, '.' | ',' | ':' | ';' | '—') {
                ' '
            } else {
                c
            }
        })
        .collect();
    cleaned
        .split_whitespace()
        .filter(|w| !matches!(*w, "and" | "then" | "you"))
        .map(|w| match w {
            "a" | "an" | "one" => "1".to_owned(),
            "two" => "2".to_owned(),
            "three" => "3".to_owned(),
            "four" => "4".to_owned(),
            "five" => "5".to_owned(),
            "six" => "6".to_owned(),
            "seven" => "7".to_owned(),
            "whenever" => "when".to_owned(),
            other => other.to_owned(),
        })
        .collect()
}

/// Whether a rendering matches an Oracle line.
pub fn matches(rendered: &str, oracle_line: &str) -> bool {
    canonical(rendered) == canonical(oracle_line)
}

pub fn render_line(l: &LineIr) -> String {
    match l {
        LineIr::Abilities(a) => render_abilities(a),
        LineIr::Spell(targets, e) => effect(e, targets),
    }
}

/// Renders abilities; consecutive keywords share a line, joined by commas.
pub fn render_abilities(abilities: &[Ability]) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut keywords: Vec<String> = Vec::new();
    for a in abilities {
        if let Ability::Keyword(k) = a {
            keywords.push(keyword(k));
            continue;
        }
        if !keywords.is_empty() {
            lines.push(keywords.join(", "));
            keywords.clear();
        }
        lines.push(ability(a));
    }
    if !keywords.is_empty() {
        lines.push(keywords.join(", "));
    }
    lines.join("\n")
}

fn ability(a: &Ability) -> String {
    match a {
        Ability::Keyword(k) => keyword(k),
        Ability::Spell(s) => effect(&s.effect, &s.targets),
        Ability::Activated(x) => {
            let mut s = format!("{}: {}", costs(&x.cost), effect(&x.effect, &x.targets));
            match x.timing {
                Timing::Sorcery => s.push_str(". activate only as a sorcery"),
                Timing::Instant => s.push_str(". activate only as an instant"),
                Timing::Any => {}
            }
            if x.once_per_turn {
                s.push_str(". activate only once each turn");
            }
            s
        }
        Ability::Triggered(t) => {
            format!("{}, {}", trigger(&t.trigger), effect(&t.effect, &t.targets))
        }
        Ability::Static(s) => static_effect(&s.effect),
        Ability::Replacement(r) => match (&r.event, &r.instead) {
            (ReplaceEvent::WouldEnter(f), Effect::EntersTapped) if f.this => {
                "~ enters tapped".into()
            }
            _ => "<replacement>".into(),
        },
        Ability::Escape(e) => format!("<escape {}>", e.function),
    }
}

fn keyword(k: &Keyword) -> String {
    match k {
        Keyword::Deathtouch => "deathtouch".into(),
        Keyword::Defender => "defender".into(),
        Keyword::DoubleStrike => "double strike".into(),
        Keyword::FirstStrike => "first strike".into(),
        Keyword::Flash => "flash".into(),
        Keyword::Flying => "flying".into(),
        Keyword::Haste => "haste".into(),
        Keyword::Hexproof => "hexproof".into(),
        Keyword::Indestructible => "indestructible".into(),
        Keyword::Lifelink => "lifelink".into(),
        Keyword::Menace => "menace".into(),
        Keyword::Reach => "reach".into(),
        Keyword::Shroud => "shroud".into(),
        Keyword::Trample => "trample".into(),
        Keyword::Vigilance => "vigilance".into(),
        Keyword::Ward(c) => format!("ward {}", costs(c)),
        Keyword::CumulativeUpkeep(c) => format!("cumulative upkeep {}", costs(c)),
        Keyword::Partner => "partner".into(),
        Keyword::PartnerWith(n) => format!("partner with {n}"),
        Keyword::Protection(f) => format!(
            "protection from {}",
            color_word(f.color.as_deref().unwrap_or(""))
        ),
    }
}

fn color_word(c: &str) -> &'static str {
    match c {
        "W" => "white",
        "U" => "blue",
        "B" => "black",
        "R" => "red",
        "G" => "green",
        _ => "?",
    }
}

fn costs(c: &[CostPart]) -> String {
    c.iter()
        .map(|p| match p {
            CostPart::Mana(m) => m.to_lowercase(),
            CostPart::Tap => "{t}".into(),
            CostPart::Untap => "{q}".into(),
            CostPart::PayLife(q) => format!("pay {} life", qty(q)),
            CostPart::Sacrifice(ObjectSet::One(ObjectRef::This)) => "sacrifice ~".into(),
            CostPart::Sacrifice(ObjectSet::ChosenFrom { filter, .. }) => {
                format!("sacrifice a {}", filter_phrase(filter, false))
            }
            CostPart::Discard(DiscardSpec::Hand) => "discard your hand".into(),
            CostPart::Discard(DiscardSpec::Chosen { count, .. }) => {
                format!("discard {} {}", qty(count), plural_word("card", count))
            }
            other => format!("<{other:?}>"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn qty(q: &Quantity) -> String {
    match q {
        Quantity::Fixed(n) => n.to_string(),
        Quantity::Expr(QuantityExpr::X) => "x".into(),
        Quantity::Expr(e) => format!("<{e:?}>"),
    }
}

fn plural_word(w: &str, q: &Quantity) -> String {
    if matches!(q, Quantity::Fixed(1)) {
        w.to_owned()
    } else {
        format!("{w}s")
    }
}

/// "another nonland creature you control", "artifact or enchantment", "creatures".
fn filter_phrase(f: &ObjectFilter, plural: bool) -> String {
    let mut words: Vec<String> = Vec::new();
    if f.other {
        words.push("another".into());
    }
    for s in &f.supertypes {
        words.push(s.clone());
    }
    for s in &f.not_supertypes {
        words.push(format!("non{s}"));
    }
    if let Some(c) = &f.color {
        words.push(color_word(c).into());
    }
    for t in &f.not_types {
        words.push(format!("non{t}"));
    }
    let s = |t: &str| {
        if plural {
            format!("{t}s")
        } else {
            t.to_owned()
        }
    };
    let noun = match f.types.as_slice() {
        [] => s("permanent"),
        [one] => s(one),
        [a, b] => format!("{} or {}", s(a), s(b)),
        many => {
            let (last, rest) = many.split_last().unwrap_or((&many[0], &[]));
            format!(
                "{}, or {}",
                rest.iter().map(|t| s(t)).collect::<Vec<_>>().join(", "),
                s(last)
            )
        }
    };
    words.push(noun);
    match f.controller {
        PlayerRel::You => words.push("you control".into()),
        PlayerRel::Opponent => words.push("an opponent controls".into()),
        PlayerRel::Any => {}
    }
    words.join(" ")
}

fn target_phrase(targets: &[TargetSpec], i: u8) -> String {
    let Some(t) = targets.get(usize::from(i)) else {
        return format!("<target {i}>");
    };
    let up_to = if t.up_to == Some(1) { "up to one " } else { "" };
    let body = match &t.what {
        TargetKind::Any => return format!("{up_to}any target"),
        TargetKind::Player(PlayerRel::Opponent) => "target opponent".to_owned(),
        TargetKind::Player(_) => "target player".to_owned(),
        TargetKind::Object(f) if f.other => {
            let mut g = f.clone();
            g.other = false;
            format!("another target {}", filter_phrase(&g, false))
        }
        TargetKind::Object(f) => format!("target {}", filter_phrase(f, false)),
        TargetKind::Spell(f) => {
            let adj = filter_phrase(f, false);
            let adj = adj
                .strip_suffix("permanent")
                .unwrap_or(&adj)
                .trim()
                .to_owned();
            if adj.is_empty() {
                "target spell".into()
            } else {
                format!("target {adj} spell")
            }
        }
    };
    format!("{up_to}{body}")
}

fn objects(o: &ObjectSet, targets: &[TargetSpec]) -> String {
    match o {
        ObjectSet::One(ObjectRef::This) => "~".into(),
        ObjectSet::One(ObjectRef::Target(i)) => target_phrase(targets, *i),
        ObjectSet::One(ObjectRef::Triggering) => "that card".into(),
        ObjectSet::All(f) => format!("all {}", filter_phrase(f, true)),
        other => format!("<{other:?}>"),
    }
}

fn player(p: &PlayerRef, targets: &[TargetSpec]) -> String {
    match p {
        PlayerRef::You => "you".into(),
        PlayerRef::Target(i) => target_phrase(targets, *i),
        PlayerRef::ControllerOf(ObjectRef::Target(_)) => "its controller".into(),
        PlayerRef::Triggering => "that player".into(),
        PlayerRef::Each => "each player".into(),
        other => format!("<{other:?}>"),
    }
}

/// Verb agreement: "you draw" / "target player draws".
fn verb(subject: &str, base: &str) -> String {
    if subject == "you" {
        base.to_owned()
    } else {
        format!("{base}s")
    }
}

fn effect(e: &Effect, t: &[TargetSpec]) -> String {
    match e {
        Effect::Seq(v) => v
            .iter()
            .map(|x| effect(x, t))
            .collect::<Vec<_>>()
            .join(". "),
        Effect::May(x) => format!("you may {}", effect(x, t)),
        Effect::DealDamage { to, amount } => {
            let to = match to {
                Recipient::Target(i) => target_phrase(t, *i),
                Recipient::Player(p) => player(p, t),
                Recipient::Object(_) => "<object>".into(),
            };
            format!("~ deals {} damage to {to}", qty(amount))
        }
        Effect::ForEachPlayer {
            players,
            effect: inner,
        } => {
            let who = match players {
                PlayerRel::Opponent => "each opponent",
                PlayerRel::You => "you",
                PlayerRel::Any => "each player",
            };
            match &**inner {
                Effect::DealDamage { amount, .. } => {
                    format!("~ deals {} damage to {who}", qty(amount))
                }
                Effect::LoseLife { amount, .. } => format!("{who} loses {} life", qty(amount)),
                Effect::Draw { amount, .. } => format!(
                    "{who} draws {} {}",
                    qty(amount),
                    plural_word("card", amount)
                ),
                other => format!("<each {other:?}>"),
            }
        }
        Effect::Destroy(o) => format!("destroy {}", objects(o, t)),
        Effect::Exile { what, .. } => format!("exile {}", objects(what, t)),
        Effect::Tap(o) => format!("tap {}", objects(o, t)),
        Effect::Untap(o) => format!("untap {}", objects(o, t)),
        Effect::Move {
            what,
            to: Destination::Hand,
        } => format!("return {} to its owner's hand", objects(what, t)),
        Effect::Counter(o) => format!("counter {}", objects(o, t)),
        Effect::Unless {
            payer,
            cost,
            effect: inner,
        } => format!(
            "{} unless {} pays {}",
            effect(inner, t),
            player(payer, t),
            costs(cost)
        ),
        Effect::Draw { who, amount } => {
            let s = player(who, t);
            format!(
                "{s} {} {} {}",
                verb(&s, "draw"),
                qty(amount),
                plural_word("card", amount)
            )
        }
        Effect::GainLife { who, amount } => {
            let s = player(who, t);
            match amount {
                Quantity::Expr(QuantityExpr::PowerOf(_)) => {
                    format!("{s} {} life equal to its power", verb(&s, "gain"))
                }
                _ => format!("{s} {} {} life", verb(&s, "gain"), qty(amount)),
            }
        }
        Effect::LoseLife { who, amount } => {
            let s = player(who, t);
            format!("{s} {} {} life", verb(&s, "lose"), qty(amount))
        }
        Effect::AddMana { mana, .. } => format!("add {}", mana_phrase(mana)),
        Effect::Scry { amount, .. } => format!("scry {}", qty(amount)),
        Effect::Shuffle(_) => "shuffle".into(),
        Effect::Search {
            filter, to, reveal, ..
        } => {
            let what = if filter.types.is_empty() && filter.supertypes.is_empty() {
                "card".to_owned()
            } else {
                format!("{} card", filter_phrase(filter, false))
            };
            let dest = match to {
                Destination::Battlefield => "put it onto the battlefield",
                _ => "put it into your hand",
            };
            format!(
                "search your library for a {what}{}, {dest}",
                if *reveal { ", reveal it" } else { "" }
            )
        }
        Effect::CreateToken {
            token: TokenSpec::Predefined(name),
            count,
            ..
        } => {
            format!(
                "create {} {} {}",
                qty(count),
                name.to_lowercase(),
                plural_word("token", count)
            )
        }
        Effect::Pump {
            what,
            power,
            toughness,
        } => {
            let v = if matches!(what, ObjectSet::All(_)) {
                "get"
            } else {
                "gets"
            };
            format!(
                "{} {v} {power:+}/{toughness:+} until end of turn",
                objects(what, t)
            )
        }
        other => format!("<{other:?}>"),
    }
}

fn mana_phrase(m: &ManaSpec) -> String {
    match m {
        ManaSpec::Fixed(s) => s.to_lowercase(),
        ManaSpec::OneOf(v) if v.len() == 2 => {
            format!("{} or {}", v[0].to_lowercase(), v[1].to_lowercase())
        }
        ManaSpec::OneOf(v) => {
            let (last, rest) = v.split_last().unwrap_or((&v[0], &[]));
            format!(
                "{}, or {}",
                rest.iter()
                    .map(|s| s.to_lowercase())
                    .collect::<Vec<_>>()
                    .join(", "),
                last.to_lowercase()
            )
        }
        ManaSpec::AnyColor(Quantity::Fixed(1)) => "one mana of any color".into(),
        ManaSpec::AnyColor(q) => format!("{} mana in any combination of colors", qty(q)),
        ManaSpec::AnyOneColor(q) => format!("{} mana of any one color", qty(q)),
        ManaSpec::CommanderIdentity => {
            "one mana of any color in your commander's color identity".into()
        }
        other => format!("<{other:?}>"),
    }
}

fn rel_subject(r: PlayerRel) -> &'static str {
    match r {
        PlayerRel::You => "you",
        PlayerRel::Opponent => "an opponent",
        PlayerRel::Any => "a player",
    }
}

fn trigger(tr: &Trigger) -> String {
    match tr {
        Trigger::Enters(f) if f.this => "when ~ enters".into(),
        Trigger::Dies(f) if f.this => "when ~ dies".into(),
        Trigger::Attacks(f) if f.this => "whenever ~ attacks".into(),
        Trigger::LeavesBattlefield(f) if f.this => "when ~ leaves the battlefield".into(),
        Trigger::DealsCombatDamageToPlayer(f) if f.this => {
            "whenever ~ deals combat damage to a player".into()
        }
        Trigger::Enters(f) => {
            let mut g = f.clone();
            let yours = g.controller == PlayerRel::You;
            g.controller = PlayerRel::Any;
            format!(
                "whenever a {} enters{}",
                filter_phrase(&g, false),
                if yours { " under your control" } else { "" }
            )
        }
        Trigger::Dies(f) => format!("whenever a {} dies", filter_phrase(f, false)),
        Trigger::Cast { by, spell, .. } => {
            let adj = filter_phrase(spell, false);
            let adj = adj
                .strip_suffix("permanent")
                .unwrap_or(&adj)
                .trim()
                .to_owned();
            let s = rel_subject(*by);
            let v = if s == "you" { "cast" } else { "casts" };
            if adj.is_empty() {
                format!("whenever {s} {v} a spell")
            } else {
                format!("whenever {s} {v} a {adj} spell")
            }
        }
        Trigger::Draws { by, .. } => {
            let s = rel_subject(*by);
            format!("whenever {s} {} a card", verb(s, "draw"))
        }
        Trigger::Discards { by } => {
            let s = rel_subject(*by);
            format!("whenever {s} {} a card", verb(s, "discard"))
        }
        Trigger::BeginningOf { step, whose } => {
            let w = if *whose == PlayerRel::You {
                "your"
            } else {
                "each"
            };
            let s = match step {
                StepName::Upkeep => "upkeep",
                StepName::Draw => "draw step",
                StepName::End => "end step",
                StepName::PrecombatMain => "precombat main phase",
                StepName::PostcombatMain => "postcombat main phase",
                StepName::Main => "main phase",
                StepName::BeginningOfCombat => "combat",
            };
            format!("at the beginning of {w} {s}")
        }
        other => format!("<{other:?}>"),
    }
}

fn static_effect(s: &StaticEffect) -> String {
    match s {
        StaticEffect::Pump {
            affected,
            power,
            toughness,
        } => format!(
            "{} get {power:+}/{toughness:+}",
            filter_phrase(affected, true)
        ),
        StaticEffect::Grant {
            affected,
            keyword: k,
        } => format!("{} have {}", filter_phrase(affected, true), keyword(k)),
        StaticEffect::DoesntUntap(f) if f.this => "~ doesn't untap during your untap step".into(),
        StaticEffect::SkipStep {
            step: StepName::Draw,
            ..
        } => "skip your draw step".into(),
        other => format!("<{other:?}>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{normalize_line, parse_line};

    fn round_trip(text: &str, name: &str, spell: bool) {
        let line = normalize_line(text, name);
        let ir = parse_line(&line, spell).unwrap_or_else(|| panic!("parse {line:?}"));
        let r = render_line(&ir);
        assert!(matches(&r, &line), "{line:?}\n rendered {r:?}");
    }

    #[test]
    fn parsed_lines_round_trip() {
        round_trip(
            "Lightning Bolt deals 3 damage to any target.",
            "Lightning Bolt",
            true,
        );
        round_trip(
            "Exile target creature. Its controller gains life equal to its power.",
            "Swords to Plowshares",
            true,
        );
        round_trip(
            "Counter target noncreature spell unless its controller pays {2}.",
            "Spell Pierce",
            true,
        );
        round_trip(
            "Destroy target artifact or enchantment. Its controller gains 4 life.",
            "Nature's Claim",
            true,
        );
        round_trip(
            "You draw two cards and lose 2 life.",
            "Night's Whisper",
            true,
        );
        round_trip(
            "Search your library for a card, put it into your hand, then shuffle.",
            "Generic Tutor",
            true,
        );
        round_trip(
            "{T}, Sacrifice this artifact: Add one mana of any color.",
            "Lotus Petal",
            false,
        );
        round_trip(
            "Whenever an opponent casts a noncreature spell, you may draw a card.",
            "X",
            false,
        );
        round_trip("Flying, vigilance", "X", false);
        round_trip("Creatures you control get +1/+1.", "X", false);
    }

    #[test]
    fn a_wrong_number_is_caught() {
        let line = normalize_line(
            "Lightning Bolt deals 3 damage to any target.",
            "Lightning Bolt",
        );
        let Some(LineIr::Spell(t, _)) = parse_line(&line, true) else {
            panic!()
        };
        let wrong = LineIr::Spell(
            t,
            Effect::DealDamage {
                to: Recipient::Target(0),
                amount: Quantity::Fixed(2),
            },
        );
        assert!(!matches(&render_line(&wrong), &line));
    }
}
