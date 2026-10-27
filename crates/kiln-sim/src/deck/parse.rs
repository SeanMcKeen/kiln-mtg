//! Decklist parsers: plain text (MTGO/Arena/Moxfield/Archidekt exports) and CSV.

use serde::Serialize;

/// Which part of the list an entry is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Commander,
    Main,
    Sideboard,
    Companion,
    Maybeboard,
}

/// One line of a decklist, before name resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeckEntry {
    pub qty: u32,
    pub name: String,
    pub section: Section,
    /// 1-based line number in the source (CSV: data row number + 1).
    pub line: usize,
    /// Marked as a commander inline (`*CMDR*` or an Archidekt `[Commander]` tag).
    pub commander_marker: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ParsedDeck {
    pub name: Option<String>,
    pub entries: Vec<DeckEntry>,
    pub warnings: Vec<String>,
}

/// Parses text or CSV, choosing by `filename` extension or content.
pub fn parse_deck(text: &str, filename: Option<&str>) -> ParsedDeck {
    let is_csv =
        filename.is_some_and(|f| f.to_ascii_lowercase().ends_with(".csv")) || looks_like_csv(text);
    if is_csv {
        parse_csv(text)
    } else {
        parse_text(text)
    }
}

fn looks_like_csv(text: &str) -> bool {
    let first = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .to_ascii_lowercase();
    first.contains(',')
        && first
            .split(',')
            .any(|c| matches!(c.trim().trim_matches('"'), "name" | "card name" | "card"))
}

fn section_for(header: &str) -> Option<Section> {
    let h = header.trim().trim_end_matches(':').trim();
    // Drop a trailing count like "Commander (1)".
    let h = match h.rfind(" (") {
        Some(i)
            if h.ends_with(')') && h[i + 2..h.len() - 1].chars().all(|c| c.is_ascii_digit()) =>
        {
            &h[..i]
        }
        _ => h,
    };
    match h.to_ascii_lowercase().as_str() {
        "commander" | "commanders" => Some(Section::Commander),
        "deck" | "main" | "mainboard" | "main deck" | "maindeck" => Some(Section::Main),
        "sideboard" | "side" => Some(Section::Sideboard),
        "companion" | "companions" => Some(Section::Companion),
        "maybeboard" | "maybe" | "considering" => Some(Section::Maybeboard),
        _ => None,
    }
}

/// Parses MTGO/Arena-style text: `qty[x] Card Name [(SET) number] [*CMDR*]`
/// with optional section headers. Lines starting with `//` or `#` are
/// comments unless they name a section.
pub fn parse_text(text: &str) -> ParsedDeck {
    let mut deck = ParsedDeck::default();
    let mut section = Section::Main;
    for (i, raw) in text.lines().enumerate() {
        let line_no = i + 1;
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.is_empty() {
            continue;
        }
        if let Some(comment) = line.strip_prefix("//").or_else(|| line.strip_prefix('#')) {
            let comment = comment.trim();
            if let Some(s) = section_for(comment) {
                section = s;
            } else if let Some(name) = comment
                .strip_prefix("Name")
                .map(|n| n.trim_start_matches([':', ' ']))
                && !name.is_empty()
            {
                deck.name = Some(name.to_owned());
            }
            continue;
        }
        if let Some(s) = section_for(line) {
            section = s;
            continue;
        }
        let (qty, rest) = split_qty(line);
        let Some(qty) = qty else {
            if line.ends_with(':')
                || line.ends_with(')') && line.contains(" (") && !line.contains('[')
            {
                // An Archidekt-style category header ("Ramp (12)", "Lands:"): content stays in its section.
                continue;
            }
            // A bare card name means one copy.
            push(&mut deck, 1, line, section, line_no);
            continue;
        };
        if qty == 0 {
            deck.warnings
                .push(format!("line {line_no}: quantity 0 for {rest:?}; ignored"));
            continue;
        }
        push(&mut deck, qty, rest, section, line_no);
    }
    deck
}

/// Splits `"4x Lightning Bolt"` / `"4 Lightning Bolt"` into `(Some(4), "Lightning Bolt")`.
fn split_qty(line: &str) -> (Option<u32>, &str) {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return (None, line);
    }
    let rest = &line[digits..];
    let rest = rest.strip_prefix(['x', 'X']).unwrap_or(rest);
    if !rest.starts_with(char::is_whitespace) {
        return (None, line);
    }
    (line[..digits].parse().ok(), rest.trim())
}

fn push(deck: &mut ParsedDeck, qty: u32, rest: &str, section: Section, line: usize) {
    let (name, commander_marker) = clean_name(rest);
    if name.is_empty() {
        deck.warnings
            .push(format!("line {line}: no card name; ignored"));
        return;
    }
    deck.entries.push(DeckEntry {
        qty,
        name,
        section,
        line,
        commander_marker,
    });
}

/// Strips export decorations: `*CMDR*`, `*F*` foil markers, Archidekt
/// `[Category]` and `^Tag^` annotations, and a trailing `(SET) number`.
fn clean_name(rest: &str) -> (String, bool) {
    let mut s = rest.trim().to_owned();
    let mut commander = false;
    loop {
        let before = s.clone();
        let lower = s.to_ascii_lowercase();
        if lower.ends_with("*cmdr*") {
            commander = true;
            s.truncate(s.len() - 6);
        } else if lower.ends_with("*f*") || lower.ends_with("*e*") {
            s.truncate(s.len() - 3);
        } else if s.ends_with(']') {
            if let Some(open) = s.rfind('[') {
                let tag = s[open + 1..s.len() - 1].to_ascii_lowercase();
                if tag.split([',', '{']).any(|t| t.trim() == "commander") {
                    commander = true;
                }
                s.truncate(open);
            }
        } else if s.ends_with('^') {
            if let Some(open) = s[..s.len() - 1].rfind('^') {
                s.truncate(open);
            }
        } else if let Some(stripped) = strip_set_suffix(&s) {
            s = stripped;
        }
        s = s.trim().to_owned();
        if s == before {
            break;
        }
    }
    (s, commander)
}

/// `"Sol Ring (CMM) 410"` → `"Sol Ring"`; `"Sol Ring (cmm)"` → `"Sol Ring"`.
fn strip_set_suffix(s: &str) -> Option<String> {
    let (head, tail) = {
        let i = s.rfind(" (")?;
        (&s[..i], &s[i + 2..])
    };
    let close = tail.find(')')?;
    let set = &tail[..close];
    let after = tail[close + 1..].trim();
    let set_ok = (2..=6).contains(&set.len()) && set.chars().all(|c| c.is_ascii_alphanumeric());
    let number_ok =
        after.is_empty() || (!after.contains(' ') && after.chars().any(|c| c.is_ascii_digit()));
    (set_ok && number_ok).then(|| head.to_owned())
}

/// Parses CSV exports with a header row. Recognized columns (case-insensitive):
/// `name`/`card name`/`card`, `quantity`/`count`/`qty`, and optionally
/// `board`/`section` and `category`/`categories` (a `Commander` value marks a commander).
pub fn parse_csv(text: &str) -> ParsedDeck {
    let mut deck = ParsedDeck::default();
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(text.as_bytes());
    let headers: Vec<String> = match rdr.headers() {
        Ok(h) => h.iter().map(str::to_ascii_lowercase).collect(),
        Err(e) => {
            deck.warnings.push(format!("CSV header unreadable: {e}"));
            return deck;
        }
    };
    let col = |names: &[&str]| headers.iter().position(|h| names.contains(&h.as_str()));
    let Some(name_col) = col(&["name", "card name", "card"]) else {
        deck.warnings
            .push("CSV has no Name column; expected a header like `Quantity,Name`".into());
        return deck;
    };
    let qty_col = col(&["quantity", "count", "qty"]);
    let board_col = col(&["board", "section"]);
    let cat_col = col(&["category", "categories"]);
    for (i, rec) in rdr.records().enumerate() {
        let line = i + 2;
        let rec = match rec {
            Ok(r) => r,
            Err(e) => {
                deck.warnings.push(format!("CSV row {line}: {e}"));
                continue;
            }
        };
        let name = rec.get(name_col).unwrap_or("").to_owned();
        if name.is_empty() {
            continue;
        }
        let qty = match qty_col.map(|c| rec.get(c).unwrap_or("1")) {
            None => 1,
            Some(q) => match q.parse() {
                Ok(n) => n,
                Err(_) => {
                    deck.warnings.push(format!(
                        "CSV row {line}: quantity {q:?} isn't a number; using 1"
                    ));
                    1
                }
            },
        };
        let section = board_col
            .and_then(|c| rec.get(c))
            .and_then(section_for)
            .unwrap_or(Section::Main);
        let commander_marker = cat_col.and_then(|c| rec.get(c)).is_some_and(|c| {
            c.split([',', ';'])
                .any(|t| t.trim().eq_ignore_ascii_case("commander"))
        });
        deck.entries.push(DeckEntry {
            qty,
            name,
            section,
            line,
            commander_marker,
        });
    }
    deck
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mtgo_style_with_sections() {
        let d = parse_text(
            "// Name: Test Deck\nCommander\n1 Vexa, Tide Scholar\n\nDeck\n1x Sol Ring (CMM) 410\n10 Wavefield\nSideboard:\n1 Glimmer Bolt\n",
        );
        assert_eq!(d.name.as_deref(), Some("Test Deck"));
        assert_eq!(d.entries.len(), 4);
        assert_eq!(d.entries[0].section, Section::Commander);
        assert_eq!(d.entries[1].name, "Sol Ring");
        assert_eq!(d.entries[2].qty, 10);
        assert_eq!(d.entries[3].section, Section::Sideboard);
    }

    #[test]
    fn markers_and_decorations() {
        let d = parse_text(
            "1 Vexa, Tide Scholar *CMDR*\n1 Sol Ring (cmm) 410 *F*\n1x Brainstorm [Draw]\n1x Kinnan, Bonder Prodigy (iko) 192 [Commander{top}]\n",
        );
        assert!(d.entries[0].commander_marker);
        assert_eq!(d.entries[0].name, "Vexa, Tide Scholar");
        assert_eq!(d.entries[1].name, "Sol Ring");
        assert_eq!(d.entries[2].name, "Brainstorm");
        assert!(!d.entries[2].commander_marker);
        assert!(d.entries[3].commander_marker);
        assert_eq!(d.entries[3].name, "Kinnan, Bonder Prodigy");
    }

    #[test]
    fn category_headers_and_bare_names() {
        let d = parse_text("Ramp (2)\n1 Sol Ring\nLands:\nWavefield\n");
        assert_eq!(
            d.entries
                .iter()
                .map(|e| e.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Sol Ring", "Wavefield"]
        );
    }

    #[test]
    fn names_with_parentheses_survive() {
        // Not a set code: contains spaces.
        let d = parse_text("1 Erase (Not the Urza's Legacy One)\n");
        assert_eq!(d.entries[0].name, "Erase (Not the Urza's Legacy One)");
    }

    #[test]
    fn csv() {
        let d = parse_deck(
            "Quantity,Name,Category\n1,Vexa,Commander\n2,\"Wavefield\",Lands\nx,Sol Ring,Ramp\n",
            None,
        );
        assert_eq!(d.entries.len(), 3);
        assert!(d.entries[0].commander_marker);
        assert_eq!(d.entries[1].qty, 2);
        assert_eq!(d.entries[2].qty, 1);
        assert_eq!(d.warnings.len(), 1);
    }
}
