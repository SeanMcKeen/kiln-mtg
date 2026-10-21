//! Identifier newtypes.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Oracle identity of a card: Scryfall's `oracle_id`. The primary card key
/// (ADR-0009). All printings of a card share one Oracle ID; rules identity is
/// Oracle-level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OracleId(pub Uuid);

impl fmt::Display for OracleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for OracleId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(Self)
    }
}

/// A player seat (CR 102.1). Seat indices are stable for the whole game,
/// including after a player leaves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerId(pub u8);

impl fmt::Display for PlayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "P{}", u16::from(self.0) + 1)
    }
}

/// A game object (CR 109.1) as a generational arena index.
///
/// An object that changes zones becomes a new object (CR 400.7), which the
/// engine models by allocating a new `ObjectId`. The generation guards against
/// reusing a slot while an old id is still held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ObjectId {
    pub index: u32,
    pub generation: u32,
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}g{}", self.index, self.generation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oracle_id_round_trips_as_plain_string() {
        let s = "a3fb7228-e76b-4e96-a40e-20b5fed75685";
        let id: OracleId = s.parse().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(id.to_string(), s);
        assert_eq!(serde_json::to_string(&id).ok(), Some(format!("\"{s}\"")));
    }

    #[test]
    fn player_display_is_one_based() {
        assert_eq!(PlayerId(0).to_string(), "P1");
    }
}
