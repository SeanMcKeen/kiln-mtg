//! Building `kiln-report/1` from game records (M7 plan S7, S9–S11). A pure
//! function of the resolved spec, the manifest and the records (sorted by
//! configuration and index), so the same run always gives the same bytes.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use kiln_protocol::report::{
    ConciseReport, ConciseWinRate, ConfigReport, DecisionStats, DeckMulligans, DeckShape,
    DeckWinRate, Dist, GameShape, REPORT_VERSION, Rate, Report, TimingStats, Versions, WinRates,
};
use kiln_protocol::spec::{Mode, PolicySpec, Seating, Spec};
use kiln_protocol::{GameRecord, Outcome, RunManifest, RunStatus};

use super::stats::{mean, median_ci, quantile, r4, wilson};
use super::{ExperimentError, OPPONENT, store};

/// The Comprehensive Rules version the engine implements.
pub const CR_VERSION: &str = "2026-06-19";

fn rate(k: u64, n: u64) -> Rate {
    let (lo, hi) = wilson(k, n);
    Rate {
        n,
        k,
        rate: if n == 0 { 0.0 } else { r4(k as f64 / n as f64) },
        ci95: [r4(lo), r4(hi)],
    }
}

/// FNV-1a, for bootstrap seeds from labels.
fn label_seed(seed: u64, label: &str) -> u64 {
    label.bytes().fold(seed ^ 0xCBF2_9CE4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3)
    })
}

fn dist(mut v: Vec<u32>, seed: u64) -> Dist {
    v.sort_unstable();
    let (lo, hi) = median_ci(&v, seed);
    Dist {
        n: v.len() as u64,
        mean: r4(mean(&v)),
        median: quantile(&v, 0.5),
        median_ci95: [lo, hi],
        p10: quantile(&v, 0.1),
        p90: quantile(&v, 0.9),
    }
}

/// The decks of a configuration, in spec order (variants after decks).
fn deck_order(spec: &Spec, records: &[&GameRecord]) -> Vec<String> {
    let mut order: Vec<String> = spec.decks.iter().map(|d| d.id.clone()).collect();
    order.extend(spec.variants.keys().cloned());
    order
        .into_iter()
        .filter(|d| records.iter().any(|r| r.seats.contains(d)))
        .collect()
}

/// Each seat's finishing position: 0 = first out; the winner is last.
fn finish_positions(r: &GameRecord) -> Vec<usize> {
    let n = r.seats.len();
    let out = r.eliminations.len();
    (0..n)
        .map(|seat| {
            if r.winner == Some(seat) {
                n - 1
            } else {
                r.eliminations
                    .iter()
                    .position(|e| e.seat == seat)
                    .unwrap_or(out.min(n.saturating_sub(2)))
            }
        })
        .collect()
}

fn config_report(spec: &Spec, config: &str, records: &[&GameRecord]) -> ConfigReport {
    let m = &spec.metrics;
    let total = records.len() as u64;
    let played: Vec<&GameRecord> = records
        .iter()
        .copied()
        .filter(|r| r.outcome != Outcome::Capped)
        .collect();
    let n_played = played.len() as u64;
    let seats = records.first().map_or(0, |r| r.seats.len());
    let decks = deck_order(spec, records);
    let seed = |label: &str| label_seed(spec.seed, &format!("{config}/{label}"));
    let deck_of_winner = |r: &GameRecord| r.winner.map(|w| r.seats[w].clone());

    let win_rates = m.win_rates.then(|| {
        let mut head_to_head: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
        for r in &played {
            if let Some(w) = deck_of_winner(r) {
                for (seat, d) in r.seats.iter().enumerate() {
                    if Some(seat) != r.winner && d != OPPONENT {
                        *head_to_head
                            .entry(d.clone())
                            .or_default()
                            .entry(w.clone())
                            .or_default() += 1;
                    }
                }
            }
        }
        let deck_rates = decks
            .iter()
            .map(|d| {
                let mine: Vec<&&GameRecord> =
                    played.iter().filter(|r| r.seats.contains(d)).collect();
                let wins = mine
                    .iter()
                    .filter(|r| deck_of_winner(r).as_deref() == Some(d.as_str()))
                    .count() as u64;
                let by_seat = (0..seats)
                    .map(|s| {
                        let here: Vec<&&&GameRecord> =
                            mine.iter().filter(|r| r.seats[s] == *d).collect();
                        let k = here.iter().filter(|r| r.winner == Some(s)).count() as u64;
                        rate(k, here.len() as u64)
                    })
                    .collect();
                let mut finish = vec![0u64; seats];
                for r in &mine {
                    if let Some(seat) = r.seats.iter().position(|x| x == d) {
                        finish[finish_positions(r)[seat]] += 1;
                    }
                }
                let win = rate(wins, mine.len() as u64);
                DeckWinRate {
                    deck: d.clone(),
                    vs_baseline: r4(win.rate - 1.0 / seats.max(1) as f64),
                    win,
                    by_seat,
                    finish,
                }
            })
            .collect();
        let by_seat = (0..seats)
            .map(|s| {
                let k = played.iter().filter(|r| r.winner == Some(s)).count() as u64;
                rate(k, n_played)
            })
            .collect();
        WinRates {
            decks: deck_rates,
            by_seat,
            head_to_head,
        }
    });

    let game_shape = m.game_shape.then(|| {
        let turns: Vec<u32> = played.iter().map(|r| r.turns).collect();
        let rounds: Vec<u32> = played
            .iter()
            .map(|r| r.turns.div_ceil(u32::try_from(seats.max(1)).unwrap_or(1)))
            .collect();
        let decks = decks
            .iter()
            .map(|d| {
                let won: Vec<&&GameRecord> = played
                    .iter()
                    .filter(|r| deck_of_winner(r).as_deref() == Some(d.as_str()))
                    .collect();
                let mut types: BTreeMap<String, u64> = BTreeMap::new();
                for r in &won {
                    if let Some(t) = &r.win_type {
                        *types.entry(t.clone()).or_default() += 1;
                    }
                }
                let loss_turns: Vec<u32> = played
                    .iter()
                    .flat_map(|r| {
                        r.eliminations
                            .iter()
                            .filter(|e| r.seats[e.seat] == *d)
                            .map(|e| e.turn)
                    })
                    .collect();
                DeckShape {
                    deck: d.clone(),
                    win_turn: (!won.is_empty())
                        .then(|| dist(won.iter().map(|r| r.turns).collect(), seed(d))),
                    win_types: types
                        .into_iter()
                        .map(|(t, k)| (t, rate(k, won.len() as u64)))
                        .collect(),
                    loss_turn: (!loss_turns.is_empty())
                        .then(|| dist(loss_turns, seed(&format!("{d}/loss")))),
                }
            })
            .collect();
        GameShape {
            turns: dist(turns, seed("turns")),
            rounds: dist(rounds, seed("rounds")),
            decks,
        }
    });

    let mulligans = if m.mulligans {
        decks
            .iter()
            .map(|d| {
                let mut taken: Vec<(u32, Option<bool>)> = Vec::new();
                for r in records {
                    for (seat, x) in r.seats.iter().enumerate() {
                        if x == d {
                            let won =
                                (r.outcome != Outcome::Capped).then_some(r.winner == Some(seat));
                            taken.push((r.mulligans[seat], won));
                        }
                    }
                }
                let most = taken.iter().map(|t| t.0).max().unwrap_or(0) as usize;
                let n = taken.len() as u64;
                DeckMulligans {
                    deck: d.clone(),
                    kept_at: (0..=most)
                        .map(|k| rate(taken.iter().filter(|t| t.0 as usize == k).count() as u64, n))
                        .collect(),
                    win_rate_by_mulligans: (0..=most)
                        .map(|k| {
                            let games: Vec<bool> = taken
                                .iter()
                                .filter(|t| t.0 as usize == k)
                                .filter_map(|t| t.1)
                                .collect();
                            rate(
                                games.iter().filter(|w| **w).count() as u64,
                                games.len() as u64,
                            )
                        })
                        .collect(),
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    let decision_stats = m.decision_stats.then(|| {
        let mut by_kind: BTreeMap<String, u64> = BTreeMap::new();
        for r in records {
            for (k, n) in r.decision_kinds.iter().flatten() {
                *by_kind.entry(k.clone()).or_default() += n;
            }
        }
        DecisionStats {
            decisions_per_game: dist(
                records
                    .iter()
                    .map(|r| u32::try_from(r.decisions).unwrap_or(u32::MAX))
                    .collect(),
                seed("decisions"),
            ),
            by_kind,
        }
    });

    let timing = m.timing.then(|| {
        let timed: Vec<&kiln_protocol::record::Timing> =
            records.iter().filter_map(|r| r.timing.as_ref()).collect();
        let total: u64 = timed.iter().map(|t| t.total_us).sum();
        let mean_us = total as f64 / timed.len().max(1) as f64;
        TimingStats {
            note: "wall-clock time: varies by machine and run (not deterministic)".into(),
            games_per_second_per_core: r4(if mean_us > 0.0 { 1e6 / mean_us } else { 0.0 }),
            mean_game_ms: r4(mean_us / 1000.0),
            policy_share_by_seat: (0..seats)
                .map(|s| {
                    let p: u64 = timed
                        .iter()
                        .map(|t| t.policy_us.get(s).copied().unwrap_or(0))
                        .sum();
                    r4(p as f64 / total.max(1) as f64)
                })
                .collect(),
        }
    });

    ConfigReport {
        config: config.to_owned(),
        games: total,
        capped: rate(total - n_played, total),
        draws: rate(
            played.iter().filter(|r| r.outcome == Outcome::Draw).count() as u64,
            n_played,
        ),
        win_rates,
        game_shape,
        mulligans,
        decision_stats,
        per_card: None,
        interaction: None,
        mana: None,
        timing,
    }
}

/// Validity warnings computed from results (Validity & Caveats).
fn result_warnings(spec: &Spec, manifest: &RunManifest, configs: &[ConfigReport]) -> Vec<String> {
    let mut w = Vec::new();
    if manifest.status != RunStatus::Complete || manifest.games_done < manifest.games_total {
        w.push(format!(
            "incomplete run: {} of {} games; run it again to resume",
            manifest.games_done, manifest.games_total
        ));
    }
    let kinds: std::collections::BTreeSet<&str> = spec
        .decks
        .iter()
        .map(|d| {
            spec.policies
                .get(&d.id)
                .or_else(|| spec.policies.get("default"))
                .map_or("greedy", PolicySpec::kind)
        })
        .collect();
    w.push(format!(
        "pilots are baseline policies ({}): results measure this play, not expert play (search AI arrives in M9)",
        kinds.into_iter().collect::<Vec<_>>().join(", ")
    ));
    if spec.seating == Seating::Fixed && spec.mode != Mode::OpeningHands {
        w.push("fixed seating: seat-order bias isn't balanced across decks".into());
    }
    for c in configs {
        if c.capped.rate >= 0.05 {
            w.push(format!(
                "{}: {:.1}% of games hit a limit and are left out of win rates; raise limits.max_turns or check for stalls",
                c.config,
                c.capped.rate * 100.0
            ));
        }
        if let Some(wr) = &c.win_rates {
            let widest = wr
                .decks
                .iter()
                .map(|d| (d.win.ci95[1] - d.win.ci95[0]) / 2.0)
                .fold(0.0, f64::max);
            if widest > 0.03 {
                w.push(format!(
                    "{}: few games: win-rate intervals are up to ±{:.1} pp (about 5,000 games give ±1.2 pp at 25%)",
                    c.config,
                    widest * 100.0
                ));
            }
            let mut overlaps = Vec::new();
            for (i, a) in wr.decks.iter().enumerate() {
                for b in &wr.decks[i + 1..] {
                    if a.win.ci95[0] <= b.win.ci95[1] && b.win.ci95[0] <= a.win.ci95[1] {
                        overlaps.push(format!("{}/{}", a.deck, b.deck));
                    }
                }
            }
            if !overlaps.is_empty() {
                w.push(format!(
                    "{}: can't rank these pairs, their intervals overlap: {}",
                    c.config,
                    overlaps.join(", ")
                ));
            }
        }
    }
    w
}

fn notable(configs: &[ConfigReport]) -> Vec<String> {
    let mut out = Vec::new();
    let Some(base) = configs.first() else {
        return out;
    };
    if let Some(wr) = &base.win_rates {
        if let (Some(first), Some(last)) = (wr.by_seat.first(), wr.by_seat.last())
            && wr.by_seat.len() > 1
        {
            out.push(format!(
                "Seat 1 wins {:.1}% vs seat {} {:.1}% ({:+.1} pp)",
                first.rate * 100.0,
                wr.by_seat.len(),
                last.rate * 100.0,
                (first.rate - last.rate) * 100.0
            ));
        }
        if let Some(best) = wr
            .decks
            .iter()
            .max_by(|a, b| a.win.rate.total_cmp(&b.win.rate))
            && wr.decks.len() > 1
        {
            out.push(format!(
                "Highest win rate: {} {:.1}% [{:.1}, {:.1}]",
                best.deck,
                best.win.rate * 100.0,
                best.win.ci95[0] * 100.0,
                best.win.ci95[1] * 100.0
            ));
        }
    }
    if let Some(gs) = &base.game_shape
        && let Some(fast) = gs
            .decks
            .iter()
            .filter_map(|d| d.win_turn.as_ref().map(|w| (d, w)))
            .min_by_key(|(_, w)| w.median)
    {
        out.push(format!(
            "Fastest median win: {} on turn {} (p10 {}, p90 {})",
            fast.0.deck, fast.1.median, fast.1.p10, fast.1.p90
        ));
    }
    out
}

fn next_hints(spec: &Spec, dir: &str, records: &[GameRecord]) -> Vec<String> {
    let mut out = vec![format!("kiln sim report {dir} --format detailed")];
    if !spec.metrics.per_card.enabled && spec.mode != Mode::OpeningHands {
        out.push(format!(
            "add `metrics: {{ per_card: {{ decks: [{}] }} }}` for card-level statistics",
            spec.decks.first().map_or("…", |d| d.id.as_str())
        ));
    }
    if let Some(fast) = records
        .iter()
        .filter(|r| r.outcome == Outcome::Win)
        .min_by_key(|r| (r.turns, r.index))
    {
        out.push(format!(
            "kiln game inspect {dir} {}  # the fastest win",
            fast.index
        ));
    }
    out
}

/// Builds the detailed report.
pub fn build(spec: &Spec, manifest: &RunManifest, records: &[GameRecord]) -> Report {
    let mut names: Vec<&str> = Vec::new();
    for r in records {
        if !names.contains(&r.config.as_str()) {
            names.push(&r.config);
        }
    }
    // The base configuration first.
    names.sort_by_key(|n| (*n != kiln_protocol::record::BASE_CONFIG, *n));
    let configs: Vec<ConfigReport> = names
        .iter()
        .map(|name| {
            let mine: Vec<&GameRecord> = records.iter().filter(|r| r.config == *name).collect();
            config_report(spec, name, &mine)
        })
        .collect();
    let mut warnings = manifest.warnings.clone();
    warnings.extend(result_warnings(spec, manifest, &configs));
    let dir = spec.outputs.dir.clone().unwrap_or_default();
    Report {
        report: REPORT_VERSION.into(),
        run_id: manifest.run.clone(),
        mode: format!("{:?}", spec.mode).to_lowercase(),
        versions: Versions {
            engine: manifest.engine.clone(),
            card_db: manifest.card_db.clone(),
            cr: CR_VERSION.into(),
            spec_hash: manifest.spec_hash.clone(),
        },
        notable: notable(&configs),
        next: next_hints(spec, &dir, records),
        warnings,
        configs,
        ab: Vec::new(),
        opening_hands: Vec::new(),
    }
}

/// The agent-facing summary of a report.
pub fn concise(r: &Report) -> ConciseReport {
    let mut win_rates = BTreeMap::new();
    let mut median_win_turn = BTreeMap::new();
    let mut win_types = BTreeMap::new();
    let mut keep_at_7 = BTreeMap::new();
    for c in &r.configs {
        if let Some(wr) = &c.win_rates {
            win_rates.insert(
                c.config.clone(),
                wr.decks
                    .iter()
                    .map(|d| ConciseWinRate {
                        deck: d.deck.clone(),
                        win_rate: d.win.rate,
                        ci95: d.win.ci95,
                        by_seat: d.by_seat.iter().map(|s| s.rate).collect(),
                    })
                    .collect(),
            );
        }
        if let Some(gs) = &c.game_shape {
            median_win_turn.insert(
                c.config.clone(),
                gs.decks
                    .iter()
                    .filter_map(|d| d.win_turn.as_ref().map(|w| (d.deck.clone(), w.median)))
                    .collect(),
            );
            win_types.insert(
                c.config.clone(),
                gs.decks
                    .iter()
                    .map(|d| {
                        (
                            d.deck.clone(),
                            d.win_types
                                .iter()
                                .map(|(t, x)| (t.clone(), x.rate))
                                .collect(),
                        )
                    })
                    .collect(),
            );
        }
        if c.config == kiln_protocol::record::BASE_CONFIG {
            for m in &c.mulligans {
                if let Some(k) = m.kept_at.first() {
                    keep_at_7.insert(m.deck.clone(), k.rate);
                }
            }
        }
    }
    ConciseReport {
        report: r.report.clone(),
        run_id: r.run_id.clone(),
        mode: r.mode.clone(),
        games: r.configs.iter().map(|c| c.games).sum(),
        capped: r.configs.iter().map(|c| c.capped.k).sum(),
        versions: r.versions.clone(),
        warnings: r.warnings.clone(),
        win_rates,
        median_win_turn,
        win_types,
        keep_at_7,
        ab: r.ab.clone(),
        opening_hands: r.opening_hands.clone(),
        notable: r.notable.clone(),
        next: r.next.clone(),
    }
}

fn pct(x: f64) -> String {
    format!("{:.1}%", x * 100.0)
}

/// A human summary.
pub fn markdown(r: &Report) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# {} ({})\n", r.run_id, r.mode);
    let v = &r.versions;
    let _ = writeln!(
        s,
        "Engine {} · card DB {} · CR {} · spec {}\n",
        v.engine, v.card_db, v.cr, v.spec_hash
    );
    if !r.warnings.is_empty() {
        let _ = writeln!(s, "## Warnings");
        for w in &r.warnings {
            let _ = writeln!(s, "- {w}");
        }
        s.push('\n');
    }
    for c in &r.configs {
        let _ = writeln!(
            s,
            "## {} — {} games, capped {}, draws {}\n",
            c.config,
            c.games,
            pct(c.capped.rate),
            pct(c.draws.rate)
        );
        if let Some(wr) = &c.win_rates {
            let seats = wr.by_seat.len();
            let _ = write!(s, "| Deck | Win rate | 95% CI | vs 1/N |");
            for i in 1..=seats {
                let _ = write!(s, " Seat {i} |");
            }
            let _ = writeln!(s, "\n|---|---|---|---|{}", "---|".repeat(seats));
            for d in &wr.decks {
                let _ = write!(
                    s,
                    "| {} | {} | {}–{} | {:+.1} pp |",
                    d.deck,
                    pct(d.win.rate),
                    pct(d.win.ci95[0]),
                    pct(d.win.ci95[1]),
                    d.vs_baseline * 100.0
                );
                for x in &d.by_seat {
                    let _ = write!(s, " {} |", pct(x.rate));
                }
                s.push('\n');
            }
            let _ = write!(s, "| *all decks* | | | |");
            for x in &wr.by_seat {
                let _ = write!(s, " {} |", pct(x.rate));
            }
            s.push_str("\n\n");
        }
        if let Some(gs) = &c.game_shape {
            let _ = writeln!(
                s,
                "Game length: median {} turns (p10 {}, p90 {}), {} rounds.\n",
                gs.turns.median, gs.turns.p10, gs.turns.p90, gs.rounds.median
            );
            let _ = writeln!(
                s,
                "| Deck | Median win turn | 95% CI | Win types |\n|---|---|---|---|"
            );
            for d in &gs.decks {
                let types: Vec<String> = d
                    .win_types
                    .iter()
                    .map(|(t, x)| format!("{t} {}", pct(x.rate)))
                    .collect();
                match &d.win_turn {
                    Some(w) => {
                        let _ = writeln!(
                            s,
                            "| {} | {} | {}–{} | {} |",
                            d.deck,
                            w.median,
                            w.median_ci95[0],
                            w.median_ci95[1],
                            types.join(", ")
                        );
                    }
                    None => {
                        let _ = writeln!(s, "| {} | — | | no wins |", d.deck);
                    }
                }
            }
            s.push('\n');
        }
        if !c.mulligans.is_empty() {
            let _ = writeln!(
                s,
                "| Deck | Kept at 7 | Win rate after 0 / 1 / 2 mulligans |\n|---|---|---|"
            );
            for m in &c.mulligans {
                let by: Vec<String> = m
                    .win_rate_by_mulligans
                    .iter()
                    .take(3)
                    .map(|x| format!("{} (n={})", pct(x.rate), x.n))
                    .collect();
                let _ = writeln!(
                    s,
                    "| {} | {} | {} |",
                    m.deck,
                    m.kept_at
                        .first()
                        .map_or_else(|| "—".into(), |k| pct(k.rate)),
                    by.join(" / ")
                );
            }
            s.push('\n');
        }
    }
    for a in &r.ab {
        let _ = writeln!(
            s,
            "A/B {} vs {}: {} → {} ({:+.1} pp, 95% CI {:+.1} to {:+.1}; {} paired games)\n",
            a.variant,
            a.base_deck,
            pct(a.base.rate),
            pct(a.variant_rate.rate),
            a.difference * 100.0,
            a.difference_ci95[0] * 100.0,
            a.difference_ci95[1] * 100.0,
            a.paired_games
        );
    }
    for o in &r.opening_hands {
        let _ = writeln!(s, "Opening hands for {} ({} hands):", o.deck, o.hands);
        for (i, k) in o.kept_at.iter().enumerate() {
            let _ = writeln!(s, "- kept at {}: {}", 7 - i, pct(k.rate));
        }
        for (f, x) in &o.features {
            let _ = writeln!(s, "- {f}: {} of kept hands", pct(x.rate));
        }
        s.push('\n');
    }
    if !r.notable.is_empty() {
        let _ = writeln!(s, "## Notable");
        for n in &r.notable {
            let _ = writeln!(s, "- {n}");
        }
        s.push('\n');
    }
    let _ = writeln!(s, "## Next");
    for n in &r.next {
        let _ = writeln!(s, "- `{n}`");
    }
    s
}

/// Rebuilds a run's report from its directory and writes `report.json`,
/// `report.md` (and `games.csv` for `outputs.tables: csv`).
pub fn write_report(dir: &Path) -> Result<Report, ExperimentError> {
    let spec_path = dir.join(store::SPEC);
    let text = std::fs::read_to_string(&spec_path)
        .map_err(|e| ExperimentError::Run(format!("{}: {e}", spec_path.display())))?;
    let spec = kiln_protocol::parse_spec(&text)?;
    let manifest = store::read_manifest(dir)?;
    let records = store::read_records(dir)?;
    let report = build(&spec, &manifest, &records);
    let write = |name: &str, body: String| {
        let p = dir.join(name);
        std::fs::write(&p, body).map_err(|e| ExperimentError::Run(format!("{}: {e}", p.display())))
    };
    write(
        "report.json",
        serde_json::to_string_pretty(&report).unwrap_or_default() + "\n",
    )?;
    write("report.md", markdown(&report))?;
    if spec.outputs.tables == kiln_protocol::spec::TableFormat::Csv {
        write("games.csv", games_csv(&records))?;
    }
    Ok(report)
}

/// The games table as CSV (seats joined with `|`).
fn games_csv(records: &[GameRecord]) -> String {
    let mut s = String::from(
        "config,index,seed,seats,outcome,winner,winner_deck,win_type,turns,mulligans,first_out,decisions,capped\n",
    );
    for r in records {
        let winner_deck = r.winner.map(|w| r.seats[w].clone()).unwrap_or_default();
        let first_out = r
            .eliminations
            .first()
            .map(|e| r.seats[e.seat].clone())
            .unwrap_or_default();
        let _ = writeln!(
            s,
            "{},{},{},{},{:?},{},{},{},{},{},{},{},{}",
            r.config,
            r.index,
            r.seed,
            r.seats.join("|"),
            r.outcome,
            r.winner.map(|w| w.to_string()).unwrap_or_default(),
            winner_deck,
            r.win_type.clone().unwrap_or_default(),
            r.turns,
            r.mulligans
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("|"),
            first_out,
            r.decisions,
            r.capped.clone().unwrap_or_default()
        );
    }
    s
}
