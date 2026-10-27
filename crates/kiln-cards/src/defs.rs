//! Card definition files: the YAML form of [`CardIr`] (the DSL candidate).
//!
//! ```yaml
//! card: Swords to Plowshares
//! faces:
//!   - abilities:
//!       - spell:
//!           targets: [{ what: { object: { types: [creature] } } }]
//!           effect:
//!             seq:
//!               - exile: { what: { one: { target: 0 } } }
//!               - gain_life:
//!                   who: { controller_of: { target: 0 } }
//!                   amount: { power_of: { target: 0 } }
//! ```
//!
//! Files never contain Oracle text (ADR-0002): the renderer lint compares
//! against Oracle text from the local card DB.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ir::{CardIr, FaceIr};

/// One card definition file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardFile {
    /// The card's Oracle name.
    pub card: String,
    /// Enums are written as single-key maps (`{ activated: … }`) rather than
    /// YAML tags (`!activated`): easier for people and LLMs to write.
    #[serde(with = "yaml_serde::with::singleton_map_recursive")]
    pub faces: Vec<FaceIr>,
}

impl CardFile {
    pub fn ir(&self) -> CardIr {
        CardIr {
            faces: self.faces.clone(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DefError {
    #[error("{path}: {message}")]
    Io { path: String, message: String },
    #[error("{path}: {message}")]
    Parse { path: String, message: String },
    #[error("{path}: {message}")]
    Invalid { path: String, message: String },
}

pub fn parse(text: &str, label: &str) -> Result<CardFile, DefError> {
    let f: CardFile = yaml_serde::from_str(text).map_err(|e| DefError::Parse {
        path: label.to_owned(),
        message: e.to_string(),
    })?;
    f.ir().validate().map_err(|message| DefError::Invalid {
        path: label.to_owned(),
        message,
    })?;
    Ok(f)
}

/// A made-up card for engine tests: printed characteristics plus IR. Real
/// cards never use this form; their characteristics come from the card DB.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyntheticCard {
    pub name: String,
    /// Mana cost such as `{1}{U}`; absent for lands.
    #[serde(default)]
    pub cost: Option<String>,
    /// Type line such as `Creature — Bear`.
    pub types: String,
    /// `[power, toughness]`.
    #[serde(default)]
    pub pt: Option<[i32; 2]>,
    #[serde(default, with = "yaml_serde::with::singleton_map_recursive")]
    pub abilities: Vec<crate::ir::Ability>,
}

/// Parses a YAML list of [`SyntheticCard`]s, validating each card's IR.
pub fn parse_synthetic(text: &str, label: &str) -> Result<Vec<SyntheticCard>, DefError> {
    let cards: Vec<SyntheticCard> = yaml_serde::from_str(text).map_err(|e| DefError::Parse {
        path: label.to_owned(),
        message: e.to_string(),
    })?;
    for c in &cards {
        let ir = CardIr {
            faces: vec![FaceIr {
                abilities: c.abilities.clone(),
            }],
        };
        ir.validate().map_err(|message| DefError::Invalid {
            path: format!("{label}: {}", c.name),
            message,
        })?;
    }
    Ok(cards)
}

/// Loads every `*.yaml` definition in a directory, sorted by file name.
pub fn load_dir(dir: &Path) -> Result<Vec<CardFile>, DefError> {
    let io = |e: std::io::Error| DefError::Io {
        path: dir.display().to_string(),
        message: e.to_string(),
    };
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map_err(io)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "yaml"))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|p| {
            let text = std::fs::read_to_string(p).map_err(|e| DefError::Io {
                path: p.display().to_string(),
                message: e.to_string(),
            })?;
            parse(&text, &p.display().to_string())
        })
        .collect()
}
