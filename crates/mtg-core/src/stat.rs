//! Printed numeric characteristics: power and toughness (CR 208), loyalty
//! (CR 209) and defense (CR 310.4).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A printed power, toughness, loyalty or defense value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PrintedStat {
    /// A plain integer such as `3` or `-1` (CR 107.1: only integers).
    Fixed(i32),
    /// `*` with an optional offset (`1+*`, `*+1`, `2-*` isn't printed). The
    /// value comes from a characteristic-defining ability (CR 208.2a).
    Star { offset: i32 },
    /// `X`, as on some planeswalkers' loyalty.
    X,
    /// Values from non-tournament cards (`*²`, `?`, `∞`, `+1`, `1.5`). Kept verbatim.
    Unsupported(String),
}

impl fmt::Display for PrintedStat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrintedStat::Fixed(n) => write!(f, "{n}"),
            PrintedStat::Star { offset: 0 } => f.write_str("*"),
            PrintedStat::Star { offset } if *offset > 0 => write!(f, "{offset}+*"),
            PrintedStat::Star { offset } => write!(f, "{offset}+*"),
            PrintedStat::X => f.write_str("X"),
            PrintedStat::Unsupported(raw) => f.write_str(raw),
        }
    }
}

impl FromStr for PrintedStat {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let stat = if s == "X" {
            PrintedStat::X
        } else if s == "*" {
            PrintedStat::Star { offset: 0 }
        } else if let Some(n) = s.strip_suffix("+*").and_then(|n| n.parse().ok()) {
            PrintedStat::Star { offset: n }
        } else if let Some(n) = s.strip_prefix("*+").and_then(|n| n.parse().ok()) {
            PrintedStat::Star { offset: n }
        } else if !s.starts_with('+') && s.parse::<i32>().is_ok() {
            PrintedStat::Fixed(s.parse().unwrap_or_default())
        } else {
            PrintedStat::Unsupported(s.to_owned())
        };
        Ok(stat)
    }
}

impl Serialize for PrintedStat {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for PrintedStat {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.parse() {
            Ok(v) => Ok(v),
            Err(never) => match never {},
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PrintedStat {
        match s.parse() {
            Ok(v) => v,
            Err(never) => match never {},
        }
    }

    #[test]
    fn forms() {
        assert_eq!(p("3"), PrintedStat::Fixed(3));
        assert_eq!(p("-1"), PrintedStat::Fixed(-1));
        assert_eq!(p("*"), PrintedStat::Star { offset: 0 });
        assert_eq!(p("1+*"), PrintedStat::Star { offset: 1 });
        assert_eq!(p("*+1"), PrintedStat::Star { offset: 1 });
        assert_eq!(p("X"), PrintedStat::X);
        assert_eq!(p("+1"), PrintedStat::Unsupported("+1".into()));
        assert_eq!(p("*²"), PrintedStat::Unsupported("*²".into()));
    }

    #[test]
    fn display_round_trip() {
        for s in ["3", "-1", "*", "1+*", "X", "∞"] {
            assert_eq!(p(s).to_string(), s);
        }
    }
}
