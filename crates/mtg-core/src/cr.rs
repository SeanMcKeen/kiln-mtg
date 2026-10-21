//! References to Comprehensive Rules numbers, such as `704.5k`.
//!
//! A reference has a three-digit rule (`704`), an optional subrule number
//! (`704.5`) and an optional subrule letter (`704.5k`). Subrule letters skip
//! "l" and "o" (see the CR introduction), so those letters are rejected.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A Comprehensive Rules reference: `903`, `903.4`, or `903.4c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CrRef {
    rule: u16,
    sub: Option<u16>,
    letter: Option<char>,
}

/// Why a string isn't a valid CR reference.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CrRefError {
    #[error("CR reference {0:?} must start with a three-digit rule number like 704")]
    BadRule(String),
    #[error("CR reference {0:?} has a malformed subrule number (expected e.g. 704.5)")]
    BadSub(String),
    #[error("CR reference {0:?} uses subrule letter {1:?}; the CR skips 'l' and 'o'")]
    SkippedLetter(String, char),
    #[error("CR reference {0:?} has trailing text (expected e.g. 704.5k)")]
    Trailing(String),
}

impl CrRef {
    /// Builds a reference from parts. Returns `None` for out-of-range rules or skipped letters.
    pub fn new(rule: u16, sub: Option<u16>, letter: Option<char>) -> Option<Self> {
        if !(100..=999).contains(&rule) {
            return None;
        }
        if letter.is_some() && sub.is_none() {
            return None;
        }
        if let Some(l) = letter
            && (!l.is_ascii_lowercase() || l == 'l' || l == 'o')
        {
            return None;
        }
        Some(Self { rule, sub, letter })
    }

    pub fn rule(self) -> u16 {
        self.rule
    }

    pub fn sub(self) -> Option<u16> {
        self.sub
    }

    pub fn letter(self) -> Option<char> {
        self.letter
    }

    /// The section (the hundreds digit): `704.5k` is in section 7.
    pub fn section(self) -> u8 {
        u8::try_from(self.rule / 100).unwrap_or(0)
    }

    /// The enclosing reference: `704.5k` → `704.5` → `704` → `None`.
    pub fn parent(self) -> Option<Self> {
        match (self.sub, self.letter) {
            (Some(_), Some(_)) => Some(Self {
                letter: None,
                ..self
            }),
            (Some(_), None) => Some(Self { sub: None, ..self }),
            _ => None,
        }
    }
}

impl fmt::Display for CrRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.rule)?;
        if let Some(s) = self.sub {
            write!(f, ".{s}")?;
        }
        if let Some(l) = self.letter {
            write!(f, "{l}")?;
        }
        Ok(())
    }
}

impl FromStr for CrRef {
    type Err = CrRefError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s.as_bytes();
        if bytes.len() < 3
            || !bytes[..3].iter().all(u8::is_ascii_digit)
            || bytes.get(3).is_some_and(u8::is_ascii_digit)
        {
            return Err(CrRefError::BadRule(s.to_owned()));
        }
        let rule: u16 = s[..3]
            .parse()
            .map_err(|_| CrRefError::BadRule(s.to_owned()))?;
        if !(100..=999).contains(&rule) {
            return Err(CrRefError::BadRule(s.to_owned()));
        }
        let rest = &s[3..];
        if rest.is_empty() {
            return Ok(Self {
                rule,
                sub: None,
                letter: None,
            });
        }
        let Some(rest) = rest.strip_prefix('.') else {
            return Err(CrRefError::Trailing(s.to_owned()));
        };
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return Err(CrRefError::BadSub(s.to_owned()));
        }
        let sub: u16 = rest[..digits]
            .parse()
            .map_err(|_| CrRefError::BadSub(s.to_owned()))?;
        let tail = &rest[digits..];
        let mut chars = tail.chars();
        match (chars.next(), chars.next()) {
            (None, _) => Ok(Self {
                rule,
                sub: Some(sub),
                letter: None,
            }),
            (Some(l), None) if l.is_ascii_lowercase() => {
                if l == 'l' || l == 'o' {
                    Err(CrRefError::SkippedLetter(s.to_owned(), l))
                } else {
                    Ok(Self {
                        rule,
                        sub: Some(sub),
                        letter: Some(l),
                    })
                }
            }
            _ => Err(CrRefError::Trailing(s.to_owned())),
        }
    }
}

impl Serialize for CrRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for CrRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Convenience macro for a compile-time-looking CR reference: `cr!("704.5k")`.
///
/// Panics at runtime on a malformed literal, so only use it with literals that
/// are covered by the citation check (`mtg dev cr-check`).
#[macro_export]
macro_rules! cr {
    ($s:literal) => {
        <$crate::cr::CrRef as ::std::str::FromStr>::from_str($s)
            .expect(concat!("valid CR reference literal: ", $s))
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn parses_all_three_forms() {
        assert_eq!(
            "903".parse::<CrRef>().map(|r| r.to_string()),
            Ok("903".into())
        );
        assert_eq!(
            "903.4".parse::<CrRef>().map(|r| r.to_string()),
            Ok("903.4".into())
        );
        assert_eq!(
            "704.5k".parse::<CrRef>().map(|r| r.to_string()),
            Ok("704.5k".into())
        );
        assert_eq!(
            "702.124".parse::<CrRef>().map(super::CrRef::sub),
            Ok(Some(124))
        );
    }

    #[test]
    fn rejects_skipped_letters() {
        // CR introduction: subrules skip "l" and "o" (704.5k is followed by 704.5m).
        assert!(matches!(
            "704.5l".parse::<CrRef>(),
            Err(CrRefError::SkippedLetter(_, 'l'))
        ));
        assert!(matches!(
            "704.5o".parse::<CrRef>(),
            Err(CrRefError::SkippedLetter(_, 'o'))
        ));
    }

    #[test]
    fn rejects_malformed() {
        for bad in [
            "", "70", "7045", "099", "704.", "704.k", "704.5K", "704.5kk", "704-5",
        ] {
            assert!(bad.parse::<CrRef>().is_err(), "{bad} should not parse");
        }
    }

    #[test]
    fn parent_chain() {
        let r: CrRef = "704.5k".parse().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.parent().map(|p| p.to_string()), Some("704.5".into()));
        assert_eq!(
            r.parent().and_then(CrRef::parent).map(|p| p.to_string()),
            Some("704".into())
        );
        assert_eq!(r.section(), 7);
    }

    #[test]
    fn ordering_is_numeric() {
        let a: CrRef = cr!("702.9");
        let b: CrRef = cr!("702.10");
        assert!(a < b);
    }

    proptest! {
        #[test]
        fn display_parse_round_trip(rule in 100u16..=999, sub in proptest::option::of(1u16..400), letter in proptest::option::of("[a-km-np-z]")) {
            let letter = if sub.is_some() { letter.and_then(|s| s.chars().next()) } else { None };
            let r = CrRef::new(rule, sub, letter).unwrap_or_else(|| panic!("constructible"));
            prop_assert_eq!(r.to_string().parse::<CrRef>(), Ok(r));
        }
    }
}
