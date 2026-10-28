//! The coverage dashboard (`docs/09 Roadmap/Milestones/M11 Full Card Pool.md`):
//! how much of a format's card pool the engine runs, sliced by what matters
//! for prioritizing work: Commander Game Changers, EDHREC popularity tiers,
//! keywords, and (when given) appearances in a corpus of real decklists. It
//! also ranks the unverified cards to work on next.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use kiln_cards::CoverageLevel;
use kiln_cards::coverage::Coverage;
use kiln_data::{CardDb, CardRecord, Legality, NameMatch};
use kiln_types::OracleId;
use serde::Serialize;

use crate::deck::{Section, parse_deck};
use crate::library::Library;

/// Verified / parsed / unsupported counts for one slice of the pool.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Tally {
    pub verified: u64,
    pub parsed: u64,
    pub unsupported: u64,
}

impl Tally {
    fn add(&mut self, level: CoverageLevel, weight: u64) {
        match level {
            CoverageLevel::Verified => self.verified += weight,
            CoverageLevel::Parsed => self.parsed += weight,
            CoverageLevel::Unsupported => self.unsupported += weight,
        }
    }

    pub fn total(&self) -> u64 {
        self.verified + self.parsed + self.unsupported
    }

    /// Verified share in percent, one decimal.
    pub fn verified_pct(&self) -> String {
        pct(self.verified, self.total())
    }

    /// Runnable (verified or parsed) share in percent.
    pub fn runnable_pct(&self) -> String {
        pct(self.verified + self.parsed, self.total())
    }
}

fn pct(k: u64, n: u64) -> String {
    if n == 0 {
        return "—".into();
    }
    // Integer arithmetic: tenths of a percent, rounded half up.
    let tenths = (k * 1000 + n / 2) / n;
    format!("{}.{}%", tenths / 10, tenths % 10)
}

/// A card to work on next, and why it's ranked where it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NextCard {
    pub name: String,
    pub coverage: String,
    /// Appearances in the decklist corpus.
    pub appearances: u64,
    pub game_changer: bool,
    pub edhrec_rank: Option<u32>,
    /// The first unsupported reason, if unsupported.
    pub reason: Option<String>,
}

/// Coverage against a corpus of decklists.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CorpusCoverage {
    pub decklists: u64,
    /// Distinct cards (basic lands left out).
    pub cards: Tally,
    /// Card appearances (each copy in each list).
    pub appearances: Tally,
    /// Names no card in the DB matches.
    pub unresolved: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Dashboard {
    pub format: String,
    pub card_db: String,
    pub all: Tally,
    pub game_changers: Tally,
    /// EDHREC rank tiers: top 100, 500, 1000, 5000.
    pub edhrec: Vec<(u32, Tally)>,
    /// The 30 most common keywords and their coverage.
    pub keywords: Vec<(String, Tally)>,
    pub corpus: Option<CorpusCoverage>,
    pub top_reasons: Vec<(String, u64)>,
    pub next: Vec<NextCard>,
}

const TIERS: [u32; 4] = [100, 500, 1000, 5000];

fn is_basic(c: &CardRecord) -> bool {
    c.faces
        .first()
        .is_some_and(|f| f.type_line.to_string().starts_with("Basic"))
}

/// Builds the dashboard for `format`, with corpus coverage when
/// `decklists` names a directory of decklist files (`.txt`, `.csv`, `.dec`).
pub fn dashboard(
    db: &CardDb,
    lib: &Library,
    format: &str,
    decklists: Option<&Path>,
    next: usize,
) -> Result<Dashboard, String> {
    // Playable in the format: legal or restricted, not banned.
    let legal = |c: &CardRecord| {
        c.layout.is_deck_card()
            && matches!(c.legality(format), Legality::Legal | Legality::Restricted)
    };
    let level = |id: OracleId| lib.coverage(id).level();
    let mut all = Tally::default();
    let mut gc = Tally::default();
    let mut tiers = [Tally::default(); 4];
    let mut keywords: BTreeMap<String, Tally> = BTreeMap::new();
    for c in db.cards().iter().filter(|c| legal(c)) {
        let l = level(c.oracle_id);
        all.add(l, 1);
        if c.game_changer {
            gc.add(l, 1);
        }
        if let Some(r) = c.edhrec_rank {
            for (i, t) in TIERS.iter().enumerate() {
                if r <= *t {
                    tiers[i].add(l, 1);
                }
            }
        }
        for k in &c.keywords {
            keywords.entry(k.clone()).or_default().add(l, 1);
        }
    }
    let mut kw: Vec<(String, Tally)> = keywords.into_iter().collect();
    kw.sort_by(|a, b| b.1.total().cmp(&a.1.total()).then(a.0.cmp(&b.0)));
    kw.truncate(30);

    // Decklist corpus: appearances by oracle id.
    let mut appearances: BTreeMap<OracleId, u64> = BTreeMap::new();
    let corpus = match decklists {
        None => None,
        Some(dir) => {
            let mut cov = CorpusCoverage::default();
            let mut files: Vec<_> = std::fs::read_dir(dir)
                .map_err(|e| format!("{}: {e}", dir.display()))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.extension()
                        .is_some_and(|x| x == "txt" || x == "csv" || x == "dec")
                })
                .collect();
            files.sort();
            for f in &files {
                let text =
                    std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?;
                let parsed = parse_deck(&text, f.file_name().and_then(|n| n.to_str()));
                cov.decklists += 1;
                for e in parsed
                    .entries
                    .iter()
                    .filter(|e| matches!(e.section, Section::Main | Section::Commander))
                {
                    match db.resolve(&e.name, format) {
                        NameMatch::Exact(i) | NameMatch::Loose(i) => {
                            let c = &db.cards()[i];
                            if !is_basic(c) && legal(c) {
                                *appearances.entry(c.oracle_id).or_default() += u64::from(e.qty);
                            }
                        }
                        _ => {
                            if !cov.unresolved.contains(&e.name) {
                                cov.unresolved.push(e.name.clone());
                            }
                        }
                    }
                }
            }
            for (id, n) in &appearances {
                let l = level(*id);
                cov.cards.add(l, 1);
                cov.appearances.add(l, *n);
            }
            cov.unresolved.sort();
            Some(cov)
        }
    };

    // Next: unverified cards by corpus appearances, then Game Changers, then EDHREC rank.
    let mut candidates: Vec<(&CardRecord, u64)> = db
        .cards()
        .iter()
        .filter(|c| legal(c) && !is_basic(c) && level(c.oracle_id) != CoverageLevel::Verified)
        .map(|c| (c, appearances.get(&c.oracle_id).copied().unwrap_or(0)))
        .collect();
    candidates.sort_by_key(|(c, n)| {
        (
            std::cmp::Reverse(*n),
            !c.game_changer,
            c.edhrec_rank.unwrap_or(u32::MAX),
            c.name.clone(),
        )
    });
    let next = candidates
        .into_iter()
        .take(next)
        .map(|(c, n)| {
            let cov = lib.coverage(c.oracle_id);
            NextCard {
                name: c.name.clone(),
                coverage: format!("{:?}", cov.level()).to_lowercase(),
                appearances: n,
                game_changer: c.game_changer,
                edhrec_rank: c.edhrec_rank,
                reason: match cov {
                    Coverage::Unsupported { reasons } => reasons.first().map(|r| r.reason.clone()),
                    _ => None,
                },
            }
        })
        .collect();
    Ok(Dashboard {
        format: format.to_owned(),
        card_db: db.id(),
        all,
        game_changers: gc,
        edhrec: TIERS.iter().copied().zip(tiers).collect(),
        keywords: kw,
        corpus,
        top_reasons: lib.top_reasons(db, legal, 15),
        next,
    })
}

impl Dashboard {
    /// The dashboard as a vault note: frontmatter dated by the card DB (not
    /// the clock, so the same data gives the same bytes), a title, the body.
    pub fn page(&self) -> String {
        let date = self
            .card_db
            .split('#')
            .next()
            .and_then(|s| s.rsplit_once('-').map(|_| s))
            .and_then(|s| s.get(s.len().saturating_sub(10)..))
            .unwrap_or("");
        format!(
            "---\ntype: reference\nstatus: generated\ntags: [cards, coverage]\nupdated: {date}\n---\n\n# Coverage Report\n\nGenerated by `kiln data coverage --dashboard --decklists tests/fixtures/decks --out \"docs/06 Card Data/Coverage Report.md\"` ([[M11 Full Card Pool]]). Regenerate after card work.\n\n{}",
            self.markdown()
        )
    }

    /// The dashboard as Markdown (no Oracle text: names and counts only).
    pub fn markdown(&self) -> String {
        let mut s = String::new();
        let row = |s: &mut String, label: &str, t: &Tally| {
            let _ = writeln!(
                s,
                "| {label} | {} | {} | {} | {} | {} | {} |",
                t.total(),
                t.verified,
                t.parsed,
                t.unsupported,
                t.verified_pct(),
                t.runnable_pct()
            );
        };
        let _ = writeln!(s, "Card DB `{}`, format `{}`.\n", self.card_db, self.format);
        let _ = writeln!(
            s,
            "Verified cards have a rules scenario proving their behaviour; parsed cards run from the Oracle-text parser without one; unsupported cards can't be played yet.\n"
        );
        let _ = writeln!(
            s,
            "| Slice | Cards | Verified | Parsed | Unsupported | Verified % | Runnable % |"
        );
        let _ = writeln!(s, "|---|---|---|---|---|---|---|");
        row(&mut s, "All legal cards", &self.all);
        row(&mut s, "Game Changers", &self.game_changers);
        for (t, tally) in &self.edhrec {
            row(&mut s, &format!("EDHREC top {t}"), tally);
        }
        if let Some(c) = &self.corpus {
            row(
                &mut s,
                &format!("Decklist corpus: distinct cards ({} lists)", c.decklists),
                &c.cards,
            );
            row(&mut s, "Decklist corpus: appearances", &c.appearances);
        }
        let _ = writeln!(
            s,
            "\n## By keyword\n\n| Keyword | Cards | Verified | Parsed | Unsupported | Verified % | Runnable % |\n|---|---|---|---|---|---|---|"
        );
        for (k, t) in &self.keywords {
            row(&mut s, k, t);
        }
        let _ = writeln!(
            s,
            "\n## Most common unsupported reasons\n\n| Cards | Reason |\n|---|---|"
        );
        for (r, n) in &self.top_reasons {
            let _ = writeln!(s, "| {n} | {} |", r.replace('|', "\\|"));
        }
        let _ = writeln!(
            s,
            "\n## Next to verify\n\n| Card | Now | In lists | Game Changer | EDHREC rank | Blocker |\n|---|---|---|---|---|---|"
        );
        for c in &self.next {
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} | {} | {} |",
                c.name,
                c.coverage,
                c.appearances,
                if c.game_changer { "★" } else { "" },
                c.edhrec_rank.map_or_else(String::new, |r| r.to_string()),
                c.reason.as_deref().unwrap_or("").replace('|', "\\|")
            );
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentages_round_half_up_in_tenths() {
        assert_eq!(pct(1, 3), "33.3%");
        assert_eq!(pct(2, 3), "66.7%");
        assert_eq!(pct(0, 0), "—");
        let t = Tally {
            verified: 19,
            parsed: 1,
            unsupported: 0,
        };
        assert_eq!(
            (t.verified_pct(), t.runnable_pct()),
            ("95.0%".into(), "100.0%".into())
        );
    }
}
