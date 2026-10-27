//! Colors (CR 105) and color sets.

use std::fmt;
use std::str::FromStr;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One of the five colors (CR 105.1), in WUBRG order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Color {
    White,
    Blue,
    Black,
    Red,
    Green,
}

impl Color {
    pub const ALL: [Color; 5] = [
        Color::White,
        Color::Blue,
        Color::Black,
        Color::Red,
        Color::Green,
    ];

    /// The mana-symbol letter for this color (CR 107.4a).
    pub fn letter(self) -> char {
        match self {
            Color::White => 'W',
            Color::Blue => 'U',
            Color::Black => 'B',
            Color::Red => 'R',
            Color::Green => 'G',
        }
    }

    pub fn from_letter(c: char) -> Option<Self> {
        match c.to_ascii_uppercase() {
            'W' => Some(Color::White),
            'U' => Some(Color::Blue),
            'B' => Some(Color::Black),
            'R' => Some(Color::Red),
            'G' => Some(Color::Green),
            _ => None,
        }
    }

    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Color::White => "white",
            Color::Blue => "blue",
            Color::Black => "black",
            Color::Red => "red",
            Color::Green => "green",
        };
        f.write_str(name)
    }
}

/// A set of colors. An object can be any subset of the five colors, including
/// none (CR 105.2). Also used for color identity (CR 903.4).
///
/// Serialized as a WUBRG letter string (`"UG"`, `""` for colorless) in
/// human-readable formats and as a bitmask byte otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Colors(u8);

impl Colors {
    pub const COLORLESS: Colors = Colors(0);
    pub const ALL: Colors = Colors(0b1_1111);

    pub fn single(c: Color) -> Self {
        Colors(c.bit())
    }

    pub fn contains(self, c: Color) -> bool {
        self.0 & c.bit() != 0
    }

    pub fn insert(&mut self, c: Color) {
        self.0 |= c.bit();
    }

    pub fn union(self, other: Colors) -> Colors {
        Colors(self.0 | other.0)
    }

    pub fn is_subset_of(self, other: Colors) -> bool {
        self.0 & !other.0 == 0
    }

    /// Colors in `self` that aren't in `other`.
    pub fn difference(self, other: Colors) -> Colors {
        Colors(self.0 & !other.0)
    }

    pub fn is_colorless(self) -> bool {
        self.0 == 0
    }

    pub fn len(self) -> u32 {
        self.0.count_ones()
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn iter(self) -> impl Iterator<Item = Color> {
        Color::ALL.into_iter().filter(move |c| self.contains(*c))
    }

    pub fn bits(self) -> u8 {
        self.0
    }
}

impl FromIterator<Color> for Colors {
    fn from_iter<I: IntoIterator<Item = Color>>(iter: I) -> Self {
        let mut s = Colors::default();
        for c in iter {
            s.insert(c);
        }
        s
    }
}

impl fmt::Display for Colors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for c in self.iter() {
            write!(f, "{}", c.letter())?;
        }
        Ok(())
    }
}

/// Parses WUBRG letters in any order; `""` and `"C"` mean colorless.
impl FromStr for Colors {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("c") {
            return Ok(Colors::COLORLESS);
        }
        s.chars()
            .map(|ch| {
                Color::from_letter(ch).ok_or_else(|| {
                    format!("{ch:?} is not a color letter (expected W, U, B, R or G)")
                })
            })
            .collect()
    }
}

impl Serialize for Colors {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.collect_str(self)
        } else {
            serializer.serialize_u8(self.0)
        }
    }
}

impl<'de> Deserialize<'de> for Colors {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            s.parse().map_err(D::Error::custom)
        } else {
            let b = u8::deserialize(deserializer)?;
            if b & !Colors::ALL.0 != 0 {
                return Err(D::Error::custom(format!("invalid color bitmask {b:#x}")));
            }
            Ok(Colors(b))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_wubrg_order() {
        let c: Colors = "GUW".parse().unwrap_or_default();
        assert_eq!(c.to_string(), "WUG");
    }

    #[test]
    fn subset() {
        let deck: Colors = "UG".parse().unwrap_or_default();
        assert!(Colors::single(Color::Green).is_subset_of(deck));
        assert!(Colors::COLORLESS.is_subset_of(deck));
        assert!(!Colors::single(Color::Red).is_subset_of(deck));
        assert_eq!(
            Colors::single(Color::Red)
                .union(deck)
                .difference(deck)
                .to_string(),
            "R"
        );
    }

    #[test]
    fn serde_both_formats() {
        let c: Colors = "WB".parse().unwrap_or_default();
        assert_eq!(serde_json::to_string(&c).ok().as_deref(), Some("\"WB\""));
        let bytes = postcard::to_stdvec(&c).unwrap_or_default();
        assert_eq!(postcard::from_bytes::<Colors>(&bytes).ok(), Some(c));
    }
}
