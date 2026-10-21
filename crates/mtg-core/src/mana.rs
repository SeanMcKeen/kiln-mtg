//! Mana symbols (CR 107.4) and mana costs (CR 202).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smallvec::SmallVec;

use crate::color::{Color, Colors};

/// One mana symbol in a cost (CR 107.4).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ManaSymbol {
    /// Numerical symbol `{0}`, `{1}`, … (CR 107.4b, 107.4d).
    Generic(u32),
    /// `{W}` `{U}` `{B}` `{R}` `{G}` (CR 107.4a).
    Colored(Color),
    /// `{C}`: payable only with colorless mana (CR 107.4c).
    Colorless,
    /// Two-color hybrid such as `{W/U}` (CR 107.4e).
    Hybrid(Color, Color),
    /// Monocolored hybrid `{2/W}`: one mana of the color or two generic (CR 107.4e).
    TwoHybrid(Color),
    /// Monocolored hybrid `{C/W}`: one colorless mana or one mana of the color (CR 107.4, 107.4e).
    ColorlessHybrid(Color),
    /// Phyrexian `{W/P}`: one mana of the color or 2 life (CR 107.4f).
    Phyrexian(Color),
    /// Hybrid Phyrexian such as `{G/U/P}` (CR 107.4f).
    HybridPhyrexian(Color, Color),
    /// Snow `{S}` (CR 107.4h).
    Snow,
    /// Variable `{X}`, `{Y}` or `{Z}` (CR 107.3).
    Variable(char),
    /// A symbol from non-tournament cards that the engine doesn't model
    /// (half mana `{½}`/`{HW}`, `{∞}`, and similar). Kept verbatim.
    Unsupported(String),
}

impl ManaSymbol {
    /// Contribution to mana value (CR 202.3). X/Y/Z count as 0 off the stack
    /// (CR 202.3e); hybrid uses its largest component (CR 202.3f); Phyrexian
    /// contributes 1 (CR 202.3g). `None` for unsupported symbols.
    pub fn mana_value(&self) -> Option<u32> {
        match self {
            ManaSymbol::Generic(n) => Some(*n),
            ManaSymbol::TwoHybrid(_) => Some(2),
            ManaSymbol::Variable(_) => Some(0),
            ManaSymbol::Colored(_)
            | ManaSymbol::Colorless
            | ManaSymbol::Hybrid(..)
            | ManaSymbol::ColorlessHybrid(_)
            | ManaSymbol::Phyrexian(_)
            | ManaSymbol::HybridPhyrexian(..)
            | ManaSymbol::Snow => Some(1),
            ManaSymbol::Unsupported(_) => None,
        }
    }

    /// The colors of this symbol. A hybrid symbol is all of its component
    /// colors (CR 107.4e); Phyrexian symbols are colored (CR 107.4f).
    pub fn colors(&self) -> Colors {
        match self {
            ManaSymbol::Colored(c)
            | ManaSymbol::TwoHybrid(c)
            | ManaSymbol::ColorlessHybrid(c)
            | ManaSymbol::Phyrexian(c) => Colors::single(*c),
            ManaSymbol::Hybrid(a, b) | ManaSymbol::HybridPhyrexian(a, b) => {
                Colors::single(*a).union(Colors::single(*b))
            }
            _ => Colors::COLORLESS,
        }
    }

    /// Parses the inside of one brace pair, e.g. `"W/U"` or `"2/B"`.
    ///
    /// Returns `None` for symbols that aren't mana symbols at all, such as
    /// `{T}`, `{Q}`, `{E}` or `{H}` (the Phyrexian symbol in rules text, CR 107.4g).
    pub fn parse_inner(inner: &str) -> Option<ManaSymbol> {
        let parts: SmallVec<[&str; 3]> = inner.split('/').collect();
        let color = |s: &str| {
            let mut it = s.chars();
            match (it.next(), it.next()) {
                (Some(c), None) => Color::from_letter(c).filter(|_| c.is_ascii_uppercase()),
                _ => None,
            }
        };
        let sym = match parts.as_slice() {
            ["C"] => ManaSymbol::Colorless,
            ["S"] => ManaSymbol::Snow,
            [v @ ("X" | "Y" | "Z")] => ManaSymbol::Variable(v.chars().next().unwrap_or('X')),
            [n] if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => {
                ManaSymbol::Generic(n.parse().ok()?)
            }
            [c] => ManaSymbol::Colored(color(c)?),
            ["2", c] => ManaSymbol::TwoHybrid(color(c)?),
            ["C", c] => ManaSymbol::ColorlessHybrid(color(c)?),
            [c, "P"] => ManaSymbol::Phyrexian(color(c)?),
            [a, b] => ManaSymbol::Hybrid(color(a)?, color(b)?),
            [a, b, "P"] => ManaSymbol::HybridPhyrexian(color(a)?, color(b)?),
            _ => return None,
        };
        Some(sym)
    }
}

impl fmt::Display for ManaSymbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManaSymbol::Generic(n) => write!(f, "{{{n}}}"),
            ManaSymbol::Colored(c) => write!(f, "{{{}}}", c.letter()),
            ManaSymbol::Colorless => f.write_str("{C}"),
            ManaSymbol::Hybrid(a, b) => write!(f, "{{{}/{}}}", a.letter(), b.letter()),
            ManaSymbol::TwoHybrid(c) => write!(f, "{{2/{}}}", c.letter()),
            ManaSymbol::ColorlessHybrid(c) => write!(f, "{{C/{}}}", c.letter()),
            ManaSymbol::Phyrexian(c) => write!(f, "{{{}/P}}", c.letter()),
            ManaSymbol::HybridPhyrexian(a, b) => write!(f, "{{{}/{}/P}}", a.letter(), b.letter()),
            ManaSymbol::Snow => f.write_str("{S}"),
            ManaSymbol::Variable(v) => write!(f, "{{{v}}}"),
            ManaSymbol::Unsupported(raw) => write!(f, "{{{raw}}}"),
        }
    }
}

/// A mana cost: an ordered list of mana symbols (CR 202.1).
///
/// "No mana cost" (CR 202.1b) is modeled as `Option<ManaCost>::None` by
/// callers; `{0}` is a present cost with a single `Generic(0)` symbol.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ManaCost {
    symbols: SmallVec<[ManaSymbol; 6]>,
}

/// Why a mana cost string didn't parse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManaCostError {
    #[error("mana cost {0:?} has text outside braces; expected symbols like {{2}}{{W}}{{U/P}}")]
    OutsideBraces(String),
    #[error("mana cost {0:?} has an unclosed brace")]
    Unclosed(String),
    #[error("mana cost {0:?} contains {{{1}}}, which isn't a mana symbol")]
    NotMana(String, String),
}

impl ManaCost {
    pub fn new(symbols: impl IntoIterator<Item = ManaSymbol>) -> Self {
        Self {
            symbols: symbols.into_iter().collect(),
        }
    }

    pub fn symbols(&self) -> &[ManaSymbol] {
        &self.symbols
    }

    /// Mana value with X = 0 (CR 202.3, 202.3e). Unsupported symbols count as 0.
    pub fn mana_value(&self) -> u32 {
        self.symbols
            .iter()
            .map(|s| s.mana_value().unwrap_or(0))
            .sum()
    }

    /// Colors implied by this cost (CR 202.2, 202.2d).
    pub fn colors(&self) -> Colors {
        self.symbols
            .iter()
            .fold(Colors::COLORLESS, |acc, s| acc.union(s.colors()))
    }

    pub fn has_unsupported(&self) -> bool {
        self.symbols
            .iter()
            .any(|s| matches!(s, ManaSymbol::Unsupported(_)))
    }

    pub fn has_variable(&self) -> bool {
        self.symbols
            .iter()
            .any(|s| matches!(s, ManaSymbol::Variable(_)))
    }
}

impl fmt::Display for ManaCost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for s in &self.symbols {
            write!(f, "{s}")?;
        }
        Ok(())
    }
}

/// Parses Oracle/Scryfall cost notation such as `{2}{W}{U/P}`.
///
/// Symbols the engine doesn't model (half mana, `{∞}`) become
/// [`ManaSymbol::Unsupported`] rather than errors, so ingestion never fails on
/// non-tournament cards. Non-mana symbols like `{T}` are errors.
impl FromStr for ManaCost {
    type Err = ManaCostError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut symbols = SmallVec::new();
        let mut rest = s.trim();
        while !rest.is_empty() {
            let Some(after_open) = rest.strip_prefix('{') else {
                return Err(ManaCostError::OutsideBraces(s.to_owned()));
            };
            let Some(close) = after_open.find('}') else {
                return Err(ManaCostError::Unclosed(s.to_owned()));
            };
            let inner = &after_open[..close];
            let sym = match ManaSymbol::parse_inner(inner) {
                Some(sym) => sym,
                None if is_unsupported_mana(inner) => ManaSymbol::Unsupported(inner.to_owned()),
                None => return Err(ManaCostError::NotMana(s.to_owned(), inner.to_owned())),
            };
            symbols.push(sym);
            rest = &after_open[close + 1..];
        }
        Ok(Self { symbols })
    }
}

/// Mana-like symbols from un-sets and joke cards.
fn is_unsupported_mana(inner: &str) -> bool {
    matches!(
        inner,
        "½" | "∞" | "HW" | "HR" | "HU" | "HB" | "HG" | "W/U/B/R/G"
    ) || inner.starts_with('H') && inner.len() == 2
        || inner.contains('/') && inner.split('/').count() > 3
}

impl Serialize for ManaCost {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ManaCost {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Colors of every mana symbol appearing in a piece of rules text, ignoring
/// non-mana symbols like `{T}` and `{H}`. Used for color identity (CR 903.4);
/// callers must strip reminder text first (CR 903.4c).
pub fn colors_in_text(text: &str) -> Colors {
    let mut colors = Colors::COLORLESS;
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };
        if let Some(sym) = ManaSymbol::parse_inner(&after[..close]) {
            colors = colors.union(sym.colors());
        }
        rest = &after[close + 1..];
    }
    colors
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn cost(s: &str) -> ManaCost {
        s.parse().unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn mana_value_rules() {
        assert_eq!(cost("{2}{W}{U}").mana_value(), 4);
        assert_eq!(cost("{X}{R}").mana_value(), 1); // CR 202.3e
        assert_eq!(cost("{2/W}{2/W}{2/W}").mana_value(), 6); // CR 202.3f
        assert_eq!(cost("{C/W}").mana_value(), 1);
        assert_eq!(cost("{W/P}{G/U/P}").mana_value(), 2); // CR 202.3g
        assert_eq!(cost("{0}").mana_value(), 0);
        assert_eq!(cost("{1000000}").mana_value(), 1_000_000);
    }

    #[test]
    fn colors_rules() {
        assert_eq!(cost("{2}{W/U}").colors().to_string(), "WU"); // CR 202.2d
        assert_eq!(cost("{2/B}{C/G}").colors().to_string(), "BG");
        assert_eq!(cost("{G/U/P}").colors().to_string(), "UG");
        assert!(cost("{4}{C}{S}").colors().is_colorless()); // CR 202.2b
    }

    #[test]
    fn unsupported_symbols_are_kept() {
        let c = cost("{½}{HW}");
        assert!(c.has_unsupported());
        assert_eq!(c.to_string(), "{½}{HW}");
    }

    #[test]
    fn rejects_non_mana() {
        assert!(matches!(
            "{T}".parse::<ManaCost>(),
            Err(ManaCostError::NotMana(..))
        ));
        assert!(matches!(
            "2W".parse::<ManaCost>(),
            Err(ManaCostError::OutsideBraces(_))
        ));
        assert!(matches!(
            "{2".parse::<ManaCost>(),
            Err(ManaCostError::Unclosed(_))
        ));
        assert!(matches!(
            "{w}".parse::<ManaCost>(),
            Err(ManaCostError::NotMana(..))
        ));
    }

    #[test]
    fn text_colors_ignore_tap_and_phyrexian_h() {
        assert_eq!(
            colors_in_text("{T}: Add {G}. {H}{2/R}: draw").to_string(),
            "RG"
        );
        assert!(colors_in_text("{T}, {Q}: Add {C}{C}.").is_colorless());
    }

    fn symbol() -> impl Strategy<Value = ManaSymbol> {
        let color = prop::sample::select(Color::ALL.to_vec());
        prop_oneof![
            (0u32..20).prop_map(ManaSymbol::Generic),
            color.clone().prop_map(ManaSymbol::Colored),
            Just(ManaSymbol::Colorless),
            (color.clone(), color.clone()).prop_map(|(a, b)| ManaSymbol::Hybrid(a, b)),
            color.clone().prop_map(ManaSymbol::TwoHybrid),
            color.clone().prop_map(ManaSymbol::ColorlessHybrid),
            color.clone().prop_map(ManaSymbol::Phyrexian),
            (color.clone(), color).prop_map(|(a, b)| ManaSymbol::HybridPhyrexian(a, b)),
            Just(ManaSymbol::Snow),
            prop::sample::select(vec!['X', 'Y', 'Z']).prop_map(ManaSymbol::Variable),
        ]
    }

    proptest! {
        #[test]
        fn display_parse_round_trip(syms in prop::collection::vec(symbol(), 0..8)) {
            let c = ManaCost::new(syms);
            prop_assert_eq!(c.to_string().parse::<ManaCost>(), Ok(c));
        }
    }
}
