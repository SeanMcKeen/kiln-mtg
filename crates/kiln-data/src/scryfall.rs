//! Raw Scryfall card objects (only the fields we use) and a bulk-file reader.
//!
//! Scryfall bulk files come as JSONL (one object per line) or as a JSON array,
//! optionally gzip-compressed. All four combinations are accepted.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct RawCard {
    pub oracle_id: Option<String>,
    pub name: String,
    pub layout: String,
    #[serde(default)]
    pub mana_cost: Option<String>,
    #[serde(default)]
    pub type_line: Option<String>,
    #[serde(default)]
    pub oracle_text: Option<String>,
    #[serde(default)]
    pub power: Option<String>,
    #[serde(default)]
    pub toughness: Option<String>,
    #[serde(default)]
    pub loyalty: Option<String>,
    #[serde(default)]
    pub defense: Option<String>,
    #[serde(default)]
    pub colors: Option<Vec<String>>,
    #[serde(default)]
    pub color_indicator: Option<Vec<String>>,
    #[serde(default)]
    pub color_identity: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub produced_mana: Option<Vec<String>>,
    #[serde(default)]
    pub legalities: BTreeMap<String, String>,
    #[serde(default)]
    pub game_changer: bool,
    #[serde(default)]
    pub reserved: bool,
    #[serde(default)]
    pub games: Vec<String>,
    #[serde(default)]
    pub set_type: Option<String>,
    #[serde(default)]
    pub edhrec_rank: Option<u32>,
    #[serde(default)]
    pub card_faces: Option<Vec<RawFace>>,
    #[serde(default)]
    pub all_parts: Option<Vec<RawPart>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawFace {
    pub name: String,
    #[serde(default)]
    pub mana_cost: Option<String>,
    #[serde(default)]
    pub type_line: Option<String>,
    #[serde(default)]
    pub oracle_text: Option<String>,
    #[serde(default)]
    pub power: Option<String>,
    #[serde(default)]
    pub toughness: Option<String>,
    #[serde(default)]
    pub loyalty: Option<String>,
    #[serde(default)]
    pub defense: Option<String>,
    #[serde(default)]
    pub colors: Option<Vec<String>>,
    #[serde(default)]
    pub color_indicator: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawPart {
    pub name: String,
    pub component: String,
}

/// A Scryfall ruling (bulk `rulings` file); only the key is needed in M1.
#[derive(Debug, Clone, Deserialize)]
pub struct RawRuling {
    pub oracle_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("can't read {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{path}: record {index} isn't a valid Scryfall object: {source}")]
    Json {
        path: String,
        index: usize,
        source: serde_json::Error,
    },
}

/// Reads every object in a Scryfall bulk file (JSONL or JSON array, gzip or not).
pub fn read_bulk<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>, ReadError> {
    let io = |source| ReadError::Io {
        path: path.display().to_string(),
        source,
    };
    let file = std::fs::File::open(path).map_err(io)?;
    let mut buffered = BufReader::new(file);
    let gz = buffered.fill_buf().map_err(io)?.starts_with(&[0x1f, 0x8b]);
    let mut reader: Box<dyn BufRead> = if gz {
        Box::new(BufReader::new(flate2::read::GzDecoder::new(buffered)))
    } else {
        Box::new(buffered)
    };
    parse_bulk(&mut reader, &path.display().to_string())
}

/// Parses bulk content from a reader. `label` names the source in errors.
pub fn parse_bulk<T: for<'de> Deserialize<'de>>(
    reader: &mut dyn BufRead,
    label: &str,
) -> Result<Vec<T>, ReadError> {
    let io = |source| ReadError::Io {
        path: label.to_owned(),
        source,
    };
    let mut first = String::new();
    // Peek at the first non-whitespace byte to tell an array from JSONL.
    loop {
        let buf = reader.fill_buf().map_err(io)?;
        if buf.is_empty() {
            return Ok(Vec::new());
        }
        let ws = buf.iter().take_while(|b| b.is_ascii_whitespace()).count();
        if ws < buf.len() {
            if buf[ws] == b'[' {
                reader.read_to_string(&mut first).map_err(io)?;
                return serde_json::from_str(&first).map_err(|source| ReadError::Json {
                    path: label.to_owned(),
                    index: 0,
                    source,
                });
            }
            break;
        }
        reader.consume(ws);
    }
    let mut out = Vec::new();
    for (i, line) in reader.lines().enumerate() {
        let line = line.map_err(io)?;
        let line = line.trim().trim_end_matches(',');
        if line.is_empty() {
            continue;
        }
        out.push(
            serde_json::from_str(line).map_err(|source| ReadError::Json {
                path: label.to_owned(),
                index: i,
                source,
            })?,
        );
    }
    Ok(out)
}
