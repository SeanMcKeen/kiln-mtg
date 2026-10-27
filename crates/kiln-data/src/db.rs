//! The versioned binary card database.
//!
//! File layout: 8-byte magic `MTGCARDB`, a little-endian `u32` format version,
//! then a postcard-encoded [`DbPayload`]. The database id is the SHA-256 of the
//! payload bytes, so identical input always yields an identical id and file.

use std::fmt;
use std::path::{Path, PathBuf};

use kiln_cards::{CardDefinition, Coverage, UnsupportedSpan};
use kiln_types::OracleId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::names::{NameIndex, NameMatch};
use crate::record::{CardRecord, Layout};

const MAGIC: &[u8; 8] = b"MTGCARDB";
/// Bump when the payload layout changes.
pub const FORMAT_VERSION: u32 = 1;
/// Bump when normalization output changes for the same input.
pub const PIPELINE_VERSION: u32 = 1;

/// What a card DB was built from (NFR-D2: stamped into every report).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardDbVersion {
    /// e.g. `scryfall/oracle_cards`.
    pub source: String,
    /// The source's own date (Scryfall `updated_at`), or `unknown`.
    pub source_date: String,
    pub pipeline_version: u32,
    /// Hash of card-definition overrides; `none` until overrides exist (M3).
    pub overrides_hash: String,
    /// SHA-256 of the input file as read.
    pub input_sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DbPayload {
    version: CardDbVersion,
    cards: Vec<CardRecord>,
}

/// Borrowed twin of [`DbPayload`] for encoding; serializes to identical bytes.
#[derive(Serialize)]
struct DbPayloadRef<'a> {
    version: &'a CardDbVersion,
    cards: &'a [CardRecord],
}

fn encode(version: &CardDbVersion, cards: &[CardRecord]) -> Result<Vec<u8>, DbError> {
    postcard::to_stdvec(&DbPayloadRef { version, cards }).map_err(DbError::Encode)
}

/// A loaded card database with its name index.
#[derive(Debug, Clone)]
pub struct CardDb {
    pub version: CardDbVersion,
    /// SHA-256 (hex) of the payload; the database's identity.
    pub content_sha256: String,
    cards: Vec<CardRecord>,
    names: NameIndex,
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("can't read card DB {path}: {source}. Build one with `kiln data build`")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("can't write card DB {path}: {source}")]
    Write {
        path: String,
        source: std::io::Error,
    },
    #[error("{path} isn't a card DB (bad magic bytes); build one with `kiln data build`")]
    BadMagic { path: String },
    #[error(
        "{path} uses card DB format v{found}, this build reads v{FORMAT_VERSION}; rebuild it with `kiln data build`"
    )]
    FormatVersion { path: String, found: u32 },
    #[error("card DB {path} is corrupt: {source}")]
    Decode {
        path: String,
        source: postcard::Error,
    },
    #[error("can't encode card DB: {0}")]
    Encode(postcard::Error),
    #[error("no card DB found under {dir}; run `kiln data build` (or pass --db PATH)")]
    NotFound { dir: String },
}

impl CardDb {
    /// Builds an in-memory DB. Cards are sorted by Oracle ID so output is
    /// independent of input order.
    pub fn new(version: CardDbVersion, mut cards: Vec<CardRecord>) -> Result<Self, DbError> {
        cards.sort_by_key(|a| a.oracle_id);
        let bytes = encode(&version, &cards)?;
        let content_sha256 = hex(&Sha256::digest(&bytes));
        let names = NameIndex::build(&cards);
        Ok(Self {
            version,
            content_sha256,
            cards,
            names,
        })
    }

    /// Short id for reports and filenames: `scryfall-2026-10-20#ab12cd34`.
    pub fn id(&self) -> String {
        format!("{}#{}", self.version.label(), &self.content_sha256[..8])
    }

    pub fn cards(&self) -> &[CardRecord] {
        &self.cards
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    pub fn names(&self) -> &NameIndex {
        &self.names
    }

    pub fn get(&self, oracle_id: OracleId) -> Option<&CardRecord> {
        self.cards
            .binary_search_by(|c| c.oracle_id.cmp(&oracle_id))
            .ok()
            .map(|i| &self.cards[i])
    }

    /// Resolves a name for `format`; indices refer to [`CardDb::cards`].
    pub fn resolve(&self, name: &str, format: &str) -> NameMatch {
        self.names.resolve(&self.cards, name, format)
    }

    /// The card's behaviour definition. M1 has no front-ends, so every card
    /// is unsupported; data-level issues are added as extra reasons.
    pub fn definition(&self, card: &CardRecord) -> CardDefinition {
        let mut extra: Vec<UnsupportedSpan> = card
            .issues
            .iter()
            .map(|i| UnsupportedSpan::whole_card(i.clone()))
            .collect();
        if let Layout::Unknown(l) = &card.layout {
            extra.retain(|s| !s.reason.contains(l.as_str()));
            extra.push(UnsupportedSpan::whole_card(format!("unknown layout {l:?}")));
        }
        CardDefinition::unsupported(card.oracle_id, extra)
    }

    pub fn coverage(&self, card: &CardRecord) -> Coverage {
        self.definition(card).coverage
    }

    /// Serializes to the binary format.
    pub fn to_bytes(&self) -> Result<Vec<u8>, DbError> {
        let body = encode(&self.version, &self.cards)?;
        let mut out = Vec::with_capacity(body.len() + 12);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&body);
        Ok(out)
    }

    pub fn from_bytes(bytes: &[u8], label: &str) -> Result<Self, DbError> {
        if bytes.len() < 12 || &bytes[..8] != MAGIC {
            return Err(DbError::BadMagic {
                path: label.to_owned(),
            });
        }
        let found = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        if found != FORMAT_VERSION {
            return Err(DbError::FormatVersion {
                path: label.to_owned(),
                found,
            });
        }
        let body = &bytes[12..];
        let payload: DbPayload = postcard::from_bytes(body).map_err(|source| DbError::Decode {
            path: label.to_owned(),
            source,
        })?;
        let content_sha256 = hex(&Sha256::digest(body));
        let names = NameIndex::build(&payload.cards);
        Ok(Self {
            version: payload.version,
            content_sha256,
            cards: payload.cards,
            names,
        })
    }

    pub fn load(path: &Path) -> Result<Self, DbError> {
        let bytes = std::fs::read(path).map_err(|source| DbError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_bytes(&bytes, &path.display().to_string())
    }

    /// Writes `carddb-<date>-<hash8>.bin` into `dir` and points `dir/current`
    /// at it. Returns the file path.
    pub fn write_to_dir(&self, dir: &Path) -> Result<PathBuf, DbError> {
        let werr = |path: &Path| {
            let path = path.display().to_string();
            move |source| DbError::Write { path, source }
        };
        std::fs::create_dir_all(dir).map_err(werr(dir))?;
        let file = dir.join(format!(
            "carddb-{}-{}.bin",
            self.version.source_date,
            &self.content_sha256[..8]
        ));
        std::fs::write(&file, self.to_bytes()?).map_err(werr(&file))?;
        let pointer = dir.join("current");
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        std::fs::write(&pointer, format!("{name}\n")).map_err(werr(&pointer))?;
        Ok(file)
    }

    /// Resolves the DB to use: an explicit path, else `dir/current`.
    pub fn locate(explicit: Option<&Path>, dir: &Path) -> Result<PathBuf, DbError> {
        if let Some(p) = explicit {
            return Ok(p.to_owned());
        }
        let pointer = dir.join("current");
        match std::fs::read_to_string(&pointer) {
            Ok(name) if !name.trim().is_empty() => Ok(dir.join(name.trim())),
            _ => Err(DbError::NotFound {
                dir: dir.display().to_string(),
            }),
        }
    }
}

impl CardDbVersion {
    /// `scryfall-2026-10-20`
    pub fn label(&self) -> String {
        let src = self.source.split('/').next().unwrap_or(&self.source);
        format!("{src}-{}", self.source_date)
    }
}

impl fmt::Display for CardDbVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (pipeline v{}, input sha256 {})",
            self.label(),
            self.pipeline_version,
            &self.input_sha256[..12.min(self.input_sha256.len())]
        )
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}
