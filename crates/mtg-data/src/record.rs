//! Normalized, rules-relevant card records (`docs/06 Card Data/Card Data Model.md`).

use std::collections::BTreeMap;
use std::fmt;

use mtg_core::{Colors, ManaCost, OracleId, PrintedStat, TypeLine};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Scryfall `layout`. Unknown values are kept, never rejected, so new layouts
/// produce unsupported records instead of crashing ingestion.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    Normal,
    Split,
    Flip,
    Transform,
    ModalDfc,
    Meld,
    Leveler,
    Class,
    Case,
    Saga,
    Adventure,
    Mutate,
    Prototype,
    Battle,
    Prepare,
    Planar,
    Scheme,
    Vanguard,
    Token,
    DoubleFacedToken,
    Emblem,
    Augment,
    Host,
    ArtSeries,
    ReversibleCard,
    FrontCard,
    Unknown(String),
}

impl Layout {
    pub fn from_scryfall(s: &str) -> Self {
        match s {
            "normal" => Layout::Normal,
            "split" => Layout::Split,
            "flip" => Layout::Flip,
            "transform" => Layout::Transform,
            "modal_dfc" => Layout::ModalDfc,
            "meld" => Layout::Meld,
            "leveler" => Layout::Leveler,
            "class" => Layout::Class,
            "case" => Layout::Case,
            "saga" => Layout::Saga,
            "adventure" => Layout::Adventure,
            "mutate" => Layout::Mutate,
            "prototype" => Layout::Prototype,
            "battle" => Layout::Battle,
            "prepare" => Layout::Prepare,
            "planar" => Layout::Planar,
            "scheme" => Layout::Scheme,
            "vanguard" => Layout::Vanguard,
            "token" => Layout::Token,
            "double_faced_token" => Layout::DoubleFacedToken,
            "emblem" => Layout::Emblem,
            "augment" => Layout::Augment,
            "host" => Layout::Host,
            "art_series" => Layout::ArtSeries,
            "reversible_card" => Layout::ReversibleCard,
            "front_card" => Layout::FrontCard,
            other => Layout::Unknown(other.to_owned()),
        }
    }

    /// Whether objects with this layout are cards that can go in a deck.
    /// Tokens, emblems, art cards, Jumpstart front cards and the
    /// Planechase/Archenemy/Vanguard oversized cards can't.
    pub fn is_deck_card(&self) -> bool {
        !matches!(
            self,
            Layout::Token
                | Layout::DoubleFacedToken
                | Layout::Emblem
                | Layout::ArtSeries
                | Layout::FrontCard
                | Layout::Planar
                | Layout::Scheme
                | Layout::Vanguard
        )
    }

    /// Whether both faces' characteristics count as the card's alternative
    /// characteristics for color identity (CR 903.4d back faces, 903.4e adventurers).
    pub fn is_multi_face(&self) -> bool {
        matches!(
            self,
            Layout::Split
                | Layout::Flip
                | Layout::Transform
                | Layout::ModalDfc
                | Layout::Adventure
                | Layout::Prepare
                | Layout::Battle
                | Layout::Meld
                | Layout::DoubleFacedToken
                | Layout::ReversibleCard
                | Layout::ArtSeries
        )
    }
}

impl fmt::Display for Layout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Layout::Unknown(s) => f.write_str(s),
            other => {
                let s = serde_json::to_value(other)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default();
                f.write_str(&s)
            }
        }
    }
}

/// Legality of a card in one format (Scryfall `legalities`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Legality {
    Legal,
    NotLegal,
    Banned,
    Restricted,
}

/// One face of a card. Single-faced cards have exactly one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FaceRecord {
    pub name: String,
    /// `None` = no mana cost (CR 202.1b), distinct from `{0}`.
    pub mana_cost: Option<ManaCost>,
    pub type_line: TypeLine,
    pub oracle_text: String,
    /// Oracle text with reminder text (parenthesized) removed (CR 207.2).
    pub reminder_stripped: String,
    /// `reminder_stripped` with the card's own name replaced by `~`.
    pub self_ref_text: String,
    pub power: Option<PrintedStat>,
    pub toughness: Option<PrintedStat>,
    pub loyalty: Option<PrintedStat>,
    pub defense: Option<PrintedStat>,
    pub colors: Colors,
    pub color_indicator: Option<Colors>,
}

/// A card-relevant entry of Scryfall `all_parts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedPart {
    pub name: String,
    /// `token`, `meld_part`, `meld_result`, `combo_piece`.
    pub component: String,
}

/// A normalized, rules-relevant card record keyed by Oracle ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardRecord {
    pub oracle_id: OracleId,
    pub name: String,
    pub layout: Layout,
    pub faces: SmallVec<[FaceRecord; 2]>,
    /// Color identity from the source (authoritative; CR 903.4).
    pub color_identity: Colors,
    /// Color identity recomputed by the pipeline, for cross-checking.
    pub computed_color_identity: Colors,
    /// Format name → legality, in sorted order.
    pub legalities: BTreeMap<String, Legality>,
    pub game_changer: bool,
    pub reserved: bool,
    pub keywords: Vec<String>,
    /// Mana letters the card can produce (`W U B R G C`), from the source.
    pub produced_mana: Vec<String>,
    /// `paper`, `mtgo`, `arena`, …
    pub games: Vec<String>,
    /// Set type of the printing the source picked (used for duplicate-name tie-breaks).
    pub set_type: String,
    pub edhrec_rank: Option<u32>,
    pub all_parts: Vec<RelatedPart>,
    /// Number of Oracle rulings, when rulings were ingested.
    pub rulings: Option<u32>,
    /// Data-level problems found during normalization (unknown layout, unparsable cost…).
    pub issues: Vec<String>,
}

impl CardRecord {
    pub fn legality(&self, format: &str) -> Legality {
        self.legalities
            .get(format)
            .copied()
            .unwrap_or(Legality::NotLegal)
    }

    /// The identity used for deck validation (CR 903.4): the union of the
    /// source's value and the recomputed one. Each catches cases the other
    /// misses: the source includes colors of basic land types (also enforced
    /// by CR 903.5d) and meld results, while recomputation catches mana
    /// symbols in rules text that the source leaves out (seen in preview cards
    /// with incomplete data). The union never accepts an off-identity card.
    pub fn deck_color_identity(&self) -> Colors {
        self.color_identity.union(self.computed_color_identity)
    }

    pub fn front(&self) -> &FaceRecord {
        &self.faces[0]
    }

    pub fn is_paper(&self) -> bool {
        self.games.iter().any(|g| g == "paper")
    }

    /// All faces' reminder-stripped text joined by newlines.
    pub fn all_text(&self) -> String {
        self.faces
            .iter()
            .map(|f| f.reminder_stripped.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// All faces' self-referencing text (name → `~`) joined by newlines.
    pub fn all_self_ref_text(&self) -> String {
        self.faces
            .iter()
            .map(|f| f.self_ref_text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
