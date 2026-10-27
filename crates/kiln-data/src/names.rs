//! Card-name normalization and the name → card index.

use std::collections::BTreeMap;

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use crate::record::{CardRecord, Layout, Legality};

/// Case-, accent- and apostrophe-insensitive form used for exact matching:
/// "Lim-Dûl’s Vault" → "lim-dul's vault".
pub fn normalize_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.nfkd() {
        if is_combining_mark(ch) {
            continue;
        }
        match ch {
            '’' | '‘' | '`' | '´' => out.push('\''),
            '“' | '”' => out.push('"'),
            'Æ' | 'æ' => out.push_str("ae"),
            '–' | '—' => out.push('-'),
            c => out.extend(c.to_lowercase()),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Punctuation- and space-insensitive form used for loose matching:
/// "Thassas Oracle" and "Thassa's Oracle" both → "thassasoracle".
pub fn loose_name(name: &str) -> String {
    normalize_name(name)
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// How a name resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameMatch {
    /// Exact match after normalization.
    Exact(usize),
    /// Matched only after ignoring punctuation and spacing; callers should warn.
    Loose(usize),
    /// Several cards share the name and the tie-breaks didn't separate them.
    Ambiguous(Vec<usize>),
    /// No match. `suggestions` are the closest card names.
    NotFound { suggestions: Vec<String> },
}

/// Name index over deck-eligible cards (see [`Layout::is_deck_card`]).
///
/// Two tiers: full card names (and the `A/B` spelling of `A // B`) always win
/// over face names. Some cards' faces reuse other cards' names (e.g. a
/// Secrets of Strixhaven prepare card whose spell face is named "Brainstorm"),
/// so "Brainstorm" must mean the card named Brainstorm.
#[derive(Debug, Clone, Default)]
pub struct NameIndex {
    exact: BTreeMap<String, Vec<usize>>,
    loose: BTreeMap<String, Vec<usize>>,
    face_exact: BTreeMap<String, Vec<usize>>,
    face_loose: BTreeMap<String, Vec<usize>>,
}

impl NameIndex {
    pub fn build(cards: &[CardRecord]) -> Self {
        let mut idx = NameIndex::default();
        for (i, card) in cards.iter().enumerate() {
            if !card.layout.is_deck_card() {
                continue;
            }
            for key in full_name_keys(card) {
                push_unique(idx.loose.entry(loose_name(&key)).or_default(), i);
                push_unique(idx.exact.entry(normalize_name(&key)).or_default(), i);
            }
            for key in face_name_keys(card) {
                push_unique(idx.face_loose.entry(loose_name(&key)).or_default(), i);
                push_unique(idx.face_exact.entry(normalize_name(&key)).or_default(), i);
            }
        }
        idx
    }

    /// Resolves a user-typed name, preferring cards that are legal in `format`
    /// (then paper, then non-joke sets) when several share the name.
    pub fn resolve(&self, cards: &[CardRecord], name: &str, format: &str) -> NameMatch {
        let name = name.trim();
        let (exact, loose) = (normalize_name(name), loose_name(name));
        let tiers = [
            (&self.exact, &exact, true),
            (&self.face_exact, &exact, true),
            (&self.loose, &loose, false),
            (&self.face_loose, &loose, false),
        ];
        for (map, key, is_exact) in tiers {
            if let Some(hits) = map.get(key.as_str()) {
                return match pick(cards, hits, format) {
                    Ok(i) if is_exact => NameMatch::Exact(i),
                    Ok(i) => NameMatch::Loose(i),
                    Err(tied) => NameMatch::Ambiguous(tied),
                };
            }
        }
        NameMatch::NotFound {
            suggestions: self.suggest(cards, name, 3),
        }
    }

    /// The closest names by Jaro-Winkler similarity on the loose form.
    pub fn suggest(&self, cards: &[CardRecord], name: &str, n: usize) -> Vec<String> {
        let target = loose_name(name);
        if target.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(u32, &str)> = self
            .loose
            .iter()
            .filter_map(|(k, hits)| {
                let score = strsim::jaro_winkler(&target, k);
                // Scores are compared as integers so ordering is deterministic.
                (score >= 0.80)
                    .then(|| ((score * 1_000_000.0) as u32, cards[hits[0]].name.as_str()))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
        let mut out: Vec<String> = Vec::new();
        for (_, name) in scored {
            if !out.iter().any(|o| o == name) {
                out.push(name.to_owned());
            }
            if out.len() == n {
                break;
            }
        }
        out
    }

    pub fn len(&self) -> usize {
        self.exact.len()
    }

    pub fn is_empty(&self) -> bool {
        self.exact.is_empty()
    }

    /// Full card names (normalized) that map to more than one card.
    pub fn duplicate_names(&self) -> impl Iterator<Item = (&str, &[usize])> {
        self.exact
            .iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    /// Face names that are shared by several cards or collide with another
    /// card's full name (they never resolve ahead of a full name).
    pub fn shadowed_face_names(&self) -> impl Iterator<Item = (&str, &[usize])> {
        self.face_exact
            .iter()
            .filter(|(k, v)| v.len() > 1 || self.exact.contains_key(k.as_str()))
            .map(|(k, v)| (k.as_str(), v.as_slice()))
    }
}

fn push_unique(v: &mut Vec<usize>, i: usize) {
    if !v.contains(&i) {
        v.push(i);
    }
}

/// A card's full name and, for `A // B` names, the `A/B` spelling.
fn full_name_keys(card: &CardRecord) -> Vec<String> {
    let mut keys = vec![card.name.clone()];
    if card.name.contains(" // ") {
        keys.push(card.name.replace(" // ", "/"));
    }
    keys
}

/// Face names a decklist may use for a multi-face card: either half of a
/// split card or either face of a double-faced card, but only the front
/// (creature) face of adventure and prepare cards, whose spell faces aren't
/// how decklists name them.
fn face_name_keys(card: &CardRecord) -> Vec<String> {
    if !card.name.contains(" // ") {
        return Vec::new();
    }
    match card.layout {
        Layout::Split | Layout::Flip | Layout::Transform | Layout::ModalDfc | Layout::Battle => {
            card.faces.iter().map(|f| f.name.clone()).collect()
        }
        Layout::Meld | Layout::ReversibleCard => Vec::new(),
        _ => card
            .faces
            .first()
            .map(|f| vec![f.name.clone()])
            .unwrap_or_default(),
    }
}

/// Duplicate-name tie-break (`docs/06 Card Data/Ingestion Pipeline.md`):
/// 1. has a legality other than `not_legal` in the target format (banned cards
///    still resolve, so validation can report them),
/// 2. playable in paper,
/// 3. not from a playtest/joke/memorabilia set.
fn pick(cards: &[CardRecord], hits: &[usize], format: &str) -> Result<usize, Vec<usize>> {
    if let [only] = hits {
        return Ok(*only);
    }
    let rank = |i: &usize| {
        let c = &cards[*i];
        (
            c.legality(format) == Legality::NotLegal,
            !c.is_paper(),
            matches!(
                c.set_type.as_str(),
                "funny" | "memorabilia" | "token" | "minigame"
            ),
        )
    };
    let best = hits.iter().map(rank).min().ok_or_else(Vec::new)?;
    let tied: Vec<usize> = hits.iter().copied().filter(|i| rank(i) == best).collect();
    match tied.as_slice() {
        [one] => Ok(*one),
        _ => Err(tied),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization() {
        assert_eq!(normalize_name("Lim-Dûl’s  Vault"), "lim-dul's vault");
        assert_eq!(normalize_name("Æther Vial"), "aether vial");
        assert_eq!(loose_name("Thassas Oracle"), loose_name("Thassa's Oracle"));
        assert_eq!(loose_name("Fire // Ice"), loose_name("fire/ice"));
    }
}
