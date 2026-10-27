//! Polite Scryfall bulk downloads (`docs/06 Card Data/Card Data Sources.md`).
//!
//! - Uses bulk files only, never per-card API calls.
//! - Sends a descriptive User-Agent and an Accept header.
//! - Caches for at least 24 hours: a fresh download is refused unless forced.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const BULK_INDEX: &str = "https://api.scryfall.com/bulk-data";
const USER_AGENT: &str = concat!(
    "kiln-mtg/",
    env!("CARGO_PKG_VERSION"),
    " (open-source rules engine; bulk-data ingestion)"
);
const MIN_AGE_SECS: u64 = 24 * 60 * 60;

/// Sidecar written next to each downloaded file (`<file>.meta.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchMeta {
    /// Scryfall bulk type, e.g. `oracle_cards`.
    pub kind: String,
    pub source_url: String,
    /// Scryfall's `updated_at` for the bulk file.
    pub updated_at: String,
    pub size: u64,
    /// Unix seconds when we downloaded it.
    pub fetched_at: u64,
}

impl FetchMeta {
    /// `YYYY-MM-DD` from `updated_at`.
    pub fn source_date(&self) -> String {
        self.updated_at.get(..10).unwrap_or("unknown").to_owned()
    }

    pub fn path_for(file: &Path) -> PathBuf {
        let mut s = file.as_os_str().to_owned();
        s.push(".meta.json");
        PathBuf::from(s)
    }

    pub fn read(file: &Path) -> Option<FetchMeta> {
        let text = std::fs::read_to_string(Self::path_for(file)).ok()?;
        serde_json::from_str(&text).ok()
    }
}

#[derive(Debug, Deserialize)]
struct BulkIndex {
    data: Vec<BulkEntry>,
}

#[derive(Debug, Deserialize)]
struct BulkEntry {
    #[serde(rename = "type")]
    kind: String,
    updated_at: String,
    download_uri: String,
    #[serde(default)]
    jsonl_download_uri: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("network error fetching {url}: {message}")]
    Http { url: String, message: String },
    #[error("unexpected bulk index format from {BULK_INDEX}: {0}")]
    Index(String),
    #[error(
        "Scryfall's bulk index has no {0:?} entry; known kinds are oracle_cards, rulings, default_cards, all_cards"
    )]
    UnknownKind(String),
    #[error("I/O error on {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
}

/// Outcome of a fetch request.
#[derive(Debug, Clone)]
pub enum Fetched {
    Downloaded {
        path: PathBuf,
        meta: FetchMeta,
    },
    /// The cached copy is newer than 24 h, so nothing was downloaded.
    CachedFresh {
        path: PathBuf,
        meta: FetchMeta,
        age_secs: u64,
    },
}

/// Downloads one Scryfall bulk file into `dir` unless a copy younger than
/// 24 h exists (or `force`). The file is `dir/<kind>.jsonl` (or `.json`).
pub fn fetch_bulk(kind: &str, dir: &Path, force: bool) -> Result<Fetched, FetchError> {
    let io = |path: &Path| {
        let path = path.display().to_string();
        move |source| FetchError::Io { path, source }
    };
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    for ext in ["jsonl", "json"] {
        let existing = dir.join(format!("{kind}.{ext}"));
        if let Some(meta) = FetchMeta::read(&existing).filter(|_| existing.exists()) {
            let age = now.saturating_sub(meta.fetched_at);
            if !force && age < MIN_AGE_SECS {
                return Ok(Fetched::CachedFresh {
                    path: existing,
                    meta,
                    age_secs: age,
                });
            }
        }
    }

    let mut index_resp = get(BULK_INDEX)?;
    let index: BulkIndex = serde_json::from_reader(index_resp.body_mut().as_reader())
        .map_err(|e| FetchError::Index(e.to_string()))?;
    let entry = index
        .data
        .into_iter()
        .find(|e| e.kind == kind)
        .ok_or_else(|| FetchError::UnknownKind(kind.to_owned()))?;
    let (url, ext): (String, &str) = match entry.jsonl_download_uri {
        Some(u) => (u, "jsonl"),
        None => (entry.download_uri, "json"),
    };
    let path = dir.join(format!("{kind}.{ext}"));
    let tmp = dir.join(format!(".{kind}.{ext}.part"));
    let mut resp = get(&url)?;
    let mut reader = resp.body_mut().with_config().limit(u64::MAX).reader();
    let mut out = std::fs::File::create(&tmp).map_err(io(&tmp))?;
    let size = std::io::copy(&mut reader, &mut out).map_err(io(&tmp))?;
    out.flush().map_err(io(&tmp))?;
    std::fs::rename(&tmp, &path).map_err(io(&path))?;
    let meta = FetchMeta {
        kind: kind.to_owned(),
        source_url: url,
        updated_at: entry.updated_at,
        size,
        fetched_at: now,
    };
    let meta_path = FetchMeta::path_for(&path);
    let json = serde_json::to_string_pretty(&meta).unwrap_or_default();
    std::fs::write(&meta_path, json).map_err(io(&meta_path))?;
    Ok(Fetched::Downloaded { path, meta })
}

fn get(url: &str) -> Result<ureq::http::Response<ureq::Body>, FetchError> {
    ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json;q=0.9,*/*;q=0.8")
        .call()
        .map_err(|e| FetchError::Http {
            url: url.to_owned(),
            message: e.to_string(),
        })
}
