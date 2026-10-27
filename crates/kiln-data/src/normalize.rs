//! Raw Scryfall objects → [`CardRecord`]s.

use std::collections::BTreeMap;

use kiln_types::mana::colors_in_text;
use kiln_types::{Color, Colors, ManaCost, OracleId, PrintedStat, TypeLine};
use smallvec::SmallVec;

use crate::record::{CardRecord, FaceRecord, Layout, Legality, RelatedPart};
use crate::scryfall::{RawCard, RawFace};

/// Converts one raw object. Returns `None` for objects with no Oracle ID
/// (Scryfall reversible-card wrappers put it on the faces); the caller counts those.
pub fn normalize(raw: &RawCard) -> Option<CardRecord> {
    let oracle_id: OracleId = raw.oracle_id.as_deref()?.parse().ok()?;
    let layout = Layout::from_scryfall(&raw.layout);
    let mut issues = Vec::new();
    if let Layout::Unknown(l) = &layout {
        issues.push(format!("unknown layout {l:?}"));
    }

    let card_colors = colors_from(raw.colors.as_deref(), &mut issues);
    let faces: SmallVec<[FaceRecord; 2]> = match &raw.card_faces {
        Some(faces) if !faces.is_empty() => faces
            .iter()
            .map(|f| face(f, card_colors, &mut issues))
            .collect(),
        _ => {
            let single = RawFace {
                name: raw.name.clone(),
                mana_cost: raw.mana_cost.clone(),
                type_line: raw.type_line.clone(),
                oracle_text: raw.oracle_text.clone(),
                power: raw.power.clone(),
                toughness: raw.toughness.clone(),
                loyalty: raw.loyalty.clone(),
                defense: raw.defense.clone(),
                colors: raw.colors.clone(),
                color_indicator: raw.color_indicator.clone(),
            };
            std::iter::once(face(&single, card_colors, &mut issues)).collect()
        }
    };

    let legalities: BTreeMap<String, Legality> = raw
        .legalities
        .iter()
        .filter_map(|(k, v)| {
            let l = match v.as_str() {
                "legal" => Legality::Legal,
                "not_legal" => Legality::NotLegal,
                "banned" => Legality::Banned,
                "restricted" => Legality::Restricted,
                other => {
                    issues.push(format!("unknown legality {other:?} for {k}"));
                    return None;
                }
            };
            Some((k.clone(), l))
        })
        .collect();

    let color_identity = colors_from(Some(&raw.color_identity), &mut issues);
    let computed_color_identity = compute_color_identity(&faces);
    let mut produced_mana = raw.produced_mana.clone().unwrap_or_default();
    produced_mana.sort_by_key(|m| "WUBRGC".find(m.as_str()).unwrap_or(9));

    Some(CardRecord {
        oracle_id,
        name: raw.name.clone(),
        layout,
        faces,
        color_identity,
        computed_color_identity,
        legalities,
        game_changer: raw.game_changer,
        reserved: raw.reserved,
        keywords: raw.keywords.clone(),
        produced_mana,
        games: raw.games.clone(),
        set_type: raw.set_type.clone().unwrap_or_default(),
        edhrec_rank: raw.edhrec_rank,
        all_parts: raw
            .all_parts
            .iter()
            .flatten()
            .filter(|p| p.component != "combo_piece" || p.name == raw.name)
            .map(|p| RelatedPart {
                name: p.name.clone(),
                component: p.component.clone(),
            })
            .collect(),
        rulings: None,
        issues,
    })
}

fn face(raw: &RawFace, card_colors: Colors, issues: &mut Vec<String>) -> FaceRecord {
    let mana_cost = match raw.mana_cost.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(s) => match s.parse::<ManaCost>() {
            Ok(c) => Some(c),
            Err(e) => {
                issues.push(format!("{}: {e}", raw.name));
                None
            }
        },
    };
    let type_line: TypeLine = match raw.type_line.as_deref().unwrap_or("").parse() {
        Ok(t) => t,
        Err(never) => match never {},
    };
    let oracle_text = raw.oracle_text.clone().unwrap_or_default();
    let reminder_stripped = strip_reminder_text(&oracle_text);
    let self_ref_text = self_reference(&reminder_stripped, &raw.name);
    let stat = |s: &Option<String>| {
        s.as_deref().map(|v| match v.parse::<PrintedStat>() {
            Ok(p) => p,
            Err(never) => match never {},
        })
    };
    FaceRecord {
        name: raw.name.clone(),
        mana_cost,
        type_line,
        oracle_text,
        reminder_stripped,
        self_ref_text,
        power: stat(&raw.power),
        toughness: stat(&raw.toughness),
        loyalty: stat(&raw.loyalty),
        defense: stat(&raw.defense),
        colors: raw
            .colors
            .as_deref()
            .map_or(card_colors, |c| colors_from(Some(c), issues)),
        color_indicator: raw
            .color_indicator
            .as_deref()
            .map(|c| colors_from(Some(c), issues)),
    }
}

fn colors_from(letters: Option<&[String]>, issues: &mut Vec<String>) -> Colors {
    let mut out = Colors::COLORLESS;
    for l in letters.unwrap_or_default() {
        match l.chars().next().and_then(Color::from_letter) {
            Some(c) if l.len() == 1 => out.insert(c),
            _ => issues.push(format!("unknown color {l:?}")),
        }
    }
    out
}

/// Removes parenthesized reminder text (CR 207.2) and tidies whitespace.
pub fn strip_reminder_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0u32;
    for ch in text.chars() {
        match ch {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Replaces a card's own name with `~`. For names with a comma ("Kinnan,
/// Bonder Prodigy"), the short name before the comma is replaced too when it
/// stands alone as a word, since Oracle text often uses it.
pub fn self_reference(text: &str, name: &str) -> String {
    let mut out = text.replace(name, "~");
    if let Some((short, _)) = name.split_once(", ")
        && short.len() > 2
    {
        out = replace_word(&out, short, "~");
    }
    out
}

fn replace_word(text: &str, word: &str, with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find(word) {
        let before_ok = rest[..pos]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let after = &rest[pos + word.len()..];
        let after_ok = after.chars().next().is_none_or(|c| !c.is_alphanumeric());
        out.push_str(&rest[..pos]);
        out.push_str(if before_ok && after_ok { with } else { word });
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Color identity per CR 903.4: mana symbols in mana costs and rules text
/// (reminder text excluded, CR 903.4c), color indicators, characteristic-
/// defining color abilities, across every face (CR 903.4d, 903.4e).
pub fn compute_color_identity(faces: &[FaceRecord]) -> Colors {
    let mut ci = Colors::COLORLESS;
    for f in faces {
        if let Some(c) = &f.mana_cost {
            ci = ci.union(c.colors());
        }
        ci = ci.union(colors_in_text(&f.reminder_stripped));
        if let Some(ind) = f.color_indicator {
            ci = ci.union(ind);
        }
        ci = ci.union(cda_colors(&f.self_ref_text));
    }
    ci
}

/// Colors set by a characteristic-defining ability (CR 604.3) such as
/// "~ is all colors." or "~ is blue and green.". Devoid ("~ is colorless")
/// adds nothing.
fn cda_colors(self_ref_text: &str) -> Colors {
    let mut out = Colors::COLORLESS;
    for line in self_ref_text.lines() {
        // e.g. "~ is all colors. This ability doesn't affect its color identity."
        if line.contains("doesn't affect its color identity") {
            continue;
        }
        let Some(rest) = line
            .strip_prefix("~ is ")
            .or_else(|| line.strip_prefix("This card is "))
        else {
            continue;
        };
        let rest = rest.trim_end_matches('.');
        if rest.starts_with("all colors") {
            return Colors::ALL;
        }
        let words: Vec<&str> = rest
            .split([' ', ','])
            .filter(|w| !w.is_empty() && *w != "and")
            .collect();
        let colors: Option<Vec<Color>> = words
            .iter()
            .map(|w| match *w {
                "white" => Some(Color::White),
                "blue" => Some(Color::Blue),
                "black" => Some(Color::Black),
                "red" => Some(Color::Red),
                "green" => Some(Color::Green),
                _ => None,
            })
            .collect();
        if let Some(cs) = colors.filter(|c| !c.is_empty()) {
            out = out.union(cs.into_iter().collect());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reminder_text_is_stripped() {
        assert_eq!(
            strip_reminder_text("Flying (This creature can't be blocked.)\nWard {2}"),
            "Flying\nWard {2}"
        );
        assert_eq!(strip_reminder_text("({T}: Add {G}.)"), "");
    }

    #[test]
    fn self_reference_handles_short_names() {
        let t = self_reference(
            "Kinnan deals damage. When Kinnan, Bonder Prodigy enters, Kinnans.",
            "Kinnan, Bonder Prodigy",
        );
        assert_eq!(t, "~ deals damage. When ~ enters, Kinnans.");
    }

    #[test]
    fn cda() {
        assert_eq!(cda_colors("~ is all colors.").to_string(), "WUBRG");
        assert_eq!(cda_colors("~ is blue and green.").to_string(), "UG");
        assert!(cda_colors("~ is colorless.").is_colorless());
        assert!(cda_colors("~ is the best").is_colorless());
    }
}
