//! Type lines: supertypes, card types and subtypes (CR 205).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

macro_rules! word_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident => $word:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The word as printed on a type line.
            pub fn word(self) -> &'static str {
                match self { $($name::$variant => $word),+ }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.word())
            }
        }

        impl FromStr for $name {
            type Err = ();

            fn from_str(s: &str) -> Result<Self, ()> {
                match s { $($word => Ok($name::$variant),)+ _ => Err(()) }
            }
        }
    };
}

word_enum! {
    /// Supertypes (CR 205.4a).
    Supertype {
        Basic => "Basic",
        Legendary => "Legendary",
        Ongoing => "Ongoing",
        Snow => "Snow",
        World => "World",
    }
}

word_enum! {
    /// Card types (CR 205.2a).
    CardType {
        Artifact => "Artifact",
        Battle => "Battle",
        Conspiracy => "Conspiracy",
        Creature => "Creature",
        Dungeon => "Dungeon",
        Enchantment => "Enchantment",
        Instant => "Instant",
        Kindred => "Kindred",
        Land => "Land",
        Phenomenon => "Phenomenon",
        Plane => "Plane",
        Planeswalker => "Planeswalker",
        Scheme => "Scheme",
        Sorcery => "Sorcery",
        Vanguard => "Vanguard",
    }
}

/// The basic land types (CR 205.3i) and the color of mana each one's
/// intrinsic ability produces (CR 305.6).
pub const BASIC_LAND_TYPES: [(&str, crate::Color); 5] = [
    ("Plains", crate::Color::White),
    ("Island", crate::Color::Blue),
    ("Swamp", crate::Color::Black),
    ("Mountain", crate::Color::Red),
    ("Forest", crate::Color::Green),
];

/// A subtype word such as `Human`, `Equipment` or `Island` (CR 205.3).
///
/// Subtypes are open-ended (new sets add creature types), so they are kept as
/// strings here. The engine will intern them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Subtype(pub String);

impl Subtype {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Subtype {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A parsed type line (CR 205.1), e.g. "Legendary Creature — Human Wizard".
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct TypeLine {
    pub supertypes: SmallVec<[Supertype; 2]>,
    pub card_types: SmallVec<[CardType; 2]>,
    pub subtypes: Vec<Subtype>,
    /// Words before the dash that are neither supertypes nor card types, such
    /// as "Token", "Emblem" or un-card types. Non-empty means the line isn't a
    /// normal tournament type line.
    pub other: Vec<String>,
}

impl TypeLine {
    pub fn has_type(&self, t: CardType) -> bool {
        self.card_types.contains(&t)
    }

    pub fn has_supertype(&self, t: Supertype) -> bool {
        self.supertypes.contains(&t)
    }

    pub fn has_subtype(&self, s: &str) -> bool {
        self.subtypes.iter().any(|x| x.0 == s)
    }

    /// The basic land types among this line's subtypes (CR 205.3i).
    pub fn basic_land_types(&self) -> impl Iterator<Item = (&'static str, crate::Color)> + '_ {
        BASIC_LAND_TYPES
            .into_iter()
            .filter(|(name, _)| self.has_subtype(name))
    }
}

impl fmt::Display for TypeLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut words: Vec<String> = self.supertypes.iter().map(ToString::to_string).collect();
        words.extend(self.other.iter().cloned());
        words.extend(self.card_types.iter().map(ToString::to_string));
        f.write_str(&words.join(" "))?;
        if !self.subtypes.is_empty() {
            let subs: Vec<&str> = self.subtypes.iter().map(Subtype::as_str).collect();
            write!(f, " — {}", subs.join(" "))?;
        }
        Ok(())
    }
}

/// Parses a single-face type line. Never fails: unknown words before the dash
/// land in [`TypeLine::other`] so ingestion can flag them.
impl FromStr for TypeLine {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (left, right) = match s.split_once('—') {
            Some((l, r)) => (l.trim(), Some(r.trim())),
            None => (s.trim(), None),
        };
        let mut line = TypeLine::default();
        for word in left.split_whitespace() {
            if let Ok(t) = word.parse::<Supertype>() {
                line.supertypes.push(t);
            } else if let Ok(t) = word.parse::<CardType>() {
                line.card_types.push(t);
            } else {
                line.other.push(word.to_owned());
            }
        }
        if let Some(right) = right.filter(|r| !r.is_empty()) {
            if line.has_type(CardType::Plane) {
                // Planar types can be several words, e.g. "The Abyss" (CR 205.3b, 205.3n).
                line.subtypes.push(Subtype(right.to_owned()));
            } else {
                let words: Vec<&str> = right.split_whitespace().collect();
                let mut i = 0;
                while i < words.len() {
                    // "Time Lord" is the one two-word creature type (CR 205.3m).
                    if words[i] == "Time" && words.get(i + 1) == Some(&"Lord") {
                        line.subtypes.push(Subtype("Time Lord".to_owned()));
                        i += 2;
                    } else {
                        line.subtypes.push(Subtype(words[i].to_owned()));
                        i += 1;
                    }
                }
            }
        }
        Ok(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tl(s: &str) -> TypeLine {
        match s.parse() {
            Ok(t) => t,
            Err(never) => match never {},
        }
    }

    #[test]
    fn legendary_creature() {
        let t = tl("Legendary Creature — Elf Druid");
        assert!(t.has_supertype(Supertype::Legendary));
        assert!(t.has_type(CardType::Creature));
        assert_eq!(t.subtypes.len(), 2);
        assert!(t.other.is_empty());
        assert_eq!(t.to_string(), "Legendary Creature — Elf Druid");
    }

    #[test]
    fn multiple_types_and_basic_land_types() {
        let t = tl("Land Creature — Forest Dryad");
        assert!(t.has_type(CardType::Land) && t.has_type(CardType::Creature));
        assert_eq!(
            t.basic_land_types().map(|(n, _)| n).collect::<Vec<_>>(),
            vec!["Forest"]
        );
        let dual = tl("Land — Island Swamp");
        assert_eq!(dual.basic_land_types().count(), 2);
    }

    #[test]
    fn time_lord_and_planes() {
        assert_eq!(
            tl("Legendary Creature — Time Lord Doctor").subtypes.len(),
            2
        );
        let plane = tl("Plane — The Abyss");
        assert_eq!(plane.subtypes, vec![Subtype("The Abyss".into())]);
    }

    #[test]
    fn unknown_words_are_collected() {
        let t = tl("Token Creature — Goblin");
        assert_eq!(t.other, vec!["Token".to_string()]);
        assert!(t.has_type(CardType::Creature));
    }

    #[test]
    fn kindred_and_no_subtypes() {
        let t = tl("Kindred Instant — Faerie");
        assert!(t.has_type(CardType::Kindred) && t.has_type(CardType::Instant));
        assert!(tl("Instant").subtypes.is_empty());
    }
}
