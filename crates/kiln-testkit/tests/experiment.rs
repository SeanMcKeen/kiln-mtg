//! Experiments on the M6 test decks (needs the card DB; skipped without it).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;

use kiln_data::CardDb;
use kiln_protocol::{GameRecord, Outcome, RunStatus};
use kiln_sim::experiment::{self, Experiment, ExperimentError, Progress, RunOptions};
use kiln_sim::library::Library;
use kiln_testkit::real::{locate_db, repo_root, verify};

fn library() -> Option<&'static (CardDb, Library)> {
    static LIB: OnceLock<Option<(CardDb, Library)>> = OnceLock::new();
    LIB.get_or_init(|| {
        let db = locate_db()?;
        let defs = kiln_cards::defs::load_dir(&repo_root().join("cards/defs")).ok()?;
        let verified = verify(&db).ok()?.verified.into_keys().collect();
        let lib = Library::build(&db, &defs, &verified);
        Some((db, lib))
    })
    .as_ref()
}

const POD: &str = r#"
spec: kiln-spec/1
name: test-pod
games: 8
seed: 7
decks:
  - { id: kinnan, source: { file: tests/fixtures/decks/kinnan-verified.txt } }
  - { id: thrasios, source: { file: tests/fixtures/decks/thrasios-tymna.txt } }
  - { id: najeela, source: { file: tests/fixtures/decks/najeela.txt } }
  - { id: rograkh, source: { file: tests/fixtures/decks/rograkh-silas.txt } }
policies:
  default: { kind: greedy }
limits: { max_turns: 40 }
"#;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kiln-exp-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn play(text: &str, dir: &Path, threads: usize) -> Result<Vec<GameRecord>, ExperimentError> {
    let Some((db, lib)) = library() else {
        return Ok(Vec::new());
    };
    let exp = Experiment::from_text(text, &repo_root(), db, lib)?;
    let cancel = AtomicBool::new(false);
    let progress = |_: Progress| {};
    let m = experiment::run(
        &exp,
        dir,
        RunOptions {
            threads: Some(threads),
            cancel: &cancel,
            on_progress: &progress,
        },
    )?;
    assert_eq!(m.status, RunStatus::Complete);
    assert_eq!(m.games_done, m.games_total);
    experiment::read_records(dir)
}

#[test]
fn pod_runs_are_deterministic_across_threads_and_resume() {
    if library().is_none() {
        return;
    }
    let (a, b, c) = (scratch("a"), scratch("b"), scratch("c"));
    let one = play(POD, &a, 1).unwrap_or_else(|e| panic!("{e}"));
    let two = play(POD, &b, 2).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(one.len(), 8);
    assert_eq!(one, two, "identical on 1 and 2 threads");
    // Balanced rotation: each deck sits in seat 1 twice.
    let firsts: Vec<&str> = one.iter().map(|r| r.seats[0].as_str()).collect();
    for d in ["kinnan", "thrasios", "najeela", "rograkh"] {
        assert_eq!(firsts.iter().filter(|x| **x == d).count(), 2, "{d}");
    }
    // Resume: keep three games and the manifest, then run again.
    std::fs::create_dir_all(&c).unwrap_or_else(|e| panic!("{e}"));
    let text = std::fs::read_to_string(a.join("games.jsonl")).unwrap_or_default();
    let first3: String = text.lines().take(3).map(|l| format!("{l}\n")).collect();
    std::fs::write(c.join("games.jsonl"), first3).unwrap_or_else(|e| panic!("{e}"));
    let resumed = play(POD, &c, 2).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(resumed, one, "a resumed run matches an uninterrupted one");
    // Reports are bit-identical for the same inputs (ADR-0005).
    let ra = experiment::write_report(&a).unwrap_or_else(|e| panic!("{e}"));
    experiment::write_report(&b).unwrap_or_else(|e| panic!("{e}"));
    let read = |d: &Path, f: &str| std::fs::read_to_string(d.join(f)).unwrap_or_default();
    assert_eq!(read(&a, "report.json"), read(&b, "report.json"));
    assert_eq!(read(&a, "report.md"), read(&b, "report.md"));
    let base = &ra.configs[0];
    assert_eq!(base.games, 8);
    let wr = base
        .win_rates
        .as_ref()
        .unwrap_or_else(|| panic!("win rates are on by default"));
    assert_eq!(wr.decks.len(), 4);
    assert_eq!(wr.by_seat.len(), 4);
    for d in &wr.decks {
        assert!(
            d.win.ci95[0] <= d.win.rate && d.win.rate <= d.win.ci95[1],
            "{d:?}"
        );
        assert_eq!(d.finish.iter().sum::<u64>(), d.win.n);
    }
    assert!(ra.warnings.iter().any(|w| w.contains("few games")));
    let concise = kiln_sim::experiment::report::concise(&ra);
    assert!(serde_json::to_string(&concise).unwrap_or_default().len() < 8000);
    for d in [a, b, c] {
        let _ = std::fs::remove_dir_all(d);
    }
}

#[test]
fn goldfish_plays_against_passive_opponents() {
    if library().is_none() {
        return;
    }
    let dir = scratch("goldfish");
    let spec = r#"
spec: kiln-spec/1
name: goldfish
mode: goldfish
games: 4
decks: [{ id: najeela, source: { file: tests/fixtures/decks/najeela.txt } }]
policies: { default: { kind: greedy } }
goldfish: { opponents: 3 }
limits: { max_turns: 40 }
"#;
    let records = play(spec, &dir, 2).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(records.len(), 4);
    for r in &records {
        assert_eq!(r.seats.iter().filter(|s| *s == "opponent").count(), 3);
        if r.outcome == Outcome::Win {
            assert_eq!(r.seats[r.winner.unwrap_or(0)], "najeela");
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_coverage_gate_refuses_unverified_cards() {
    if library().is_none() {
        return;
    }
    let spec = POD.replace("kinnan-verified.txt", "kinnan-unsupported.txt");
    let err = play(&spec, &scratch("gate"), 1)
        .expect_err("kinnan-unsupported.txt has an unsupported card");
    assert_eq!(err.exit_code(), 3, "{err}");
}

#[test]
fn opening_hands_ab_and_event_metrics() {
    if library().is_none() {
        return;
    }
    let hands = r#"
spec: kiln-spec/1
name: hands
mode: opening_hands
games: 20
decks: [{ id: kinnan, source: { file: tests/fixtures/decks/kinnan-verified.txt } }]
opening_hands: { features: { fast_mana: [Sol Ring, Mana Vault, Chrome Mox] } }
"#;
    let dir = scratch("hands");
    let records = play(hands, &dir, 2).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(records.len(), 20);
    for r in &records {
        assert_eq!(r.outcome, Outcome::NotPlayed);
        // London mulligan (CR 103.5) in a 4-player pod: one card to the bottom
        // per mulligan after the first, which is free (CR 103.5c).
        assert_eq!(r.seats.len(), 4);
        let kept = r.hand.as_ref().map_or(0, Vec::len);
        let bottom = (r.mulligans[0] as usize).saturating_sub(1);
        assert_eq!(kept, 7 - bottom, "{r:?}");
    }
    let report = experiment::write_report(&dir).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report.opening_hands.len(), 1);
    assert_eq!(report.opening_hands[0].hands, 20);
    let sizes = &report.opening_hands[0].hand_sizes;
    assert_eq!(sizes.len(), report.opening_hands[0].kept_at.len());
    assert!(sizes.iter().take(2).all(|s| *s == 7), "{sizes:?}");
    assert!(report.configs.is_empty());

    let ab = POD
        .replace("name: test-pod", "name: ab")
        .replace("games: 8", "mode: ab\ngames: 8")
        .replace(
            "policies:",
            "variants:\n  najeela_ring: { base: najeela, swap: { out: [Abrupt Decay], in: [Sol Ring] } }\nmetrics: { per_card: { decks: [kinnan], min_samples: 1 }, interaction: true, mana: true }\npolicies:",
        );
    let dir2 = scratch("ab");
    let records = play(&ab, &dir2, 2).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        records.len(),
        16,
        "8 games for the base and 8 for the variant"
    );
    assert!(
        records
            .iter()
            .all(|r| r.mana.is_some() && r.interaction.is_some())
    );
    assert!(
        records
            .iter()
            .filter(|r| r.outcome != Outcome::Capped)
            .all(|r| r.cards.as_ref().is_some_and(|c| c.len() == 1))
    );
    let report = experiment::write_report(&dir2).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report.ab.len(), 1);
    assert!(report.ab[0].paired_games <= 8);
    let base = &report.configs[0];
    assert!(base.per_card.as_ref().is_some_and(|p| p.decks.len() == 1));
    assert!(base.mana.as_ref().is_some_and(|m| m.len() == 4));
    for d in [dir, dir2] {
        let _ = std::fs::remove_dir_all(d);
    }
}

#[test]
fn playbook_policies_answer_and_report() {
    if library().is_none() {
        return;
    }
    let spec = POD.replace(
        "default: { kind: greedy }",
        "default: { kind: playbook, playbook: examples/playbooks/generic-cedh.rhai, fallback: { kind: greedy } }",
    );
    let dir = scratch("playbook");
    let records = play(&spec, &dir, 2).unwrap_or_else(|e| panic!("{e}"));
    let mut answered = 0;
    for r in &records {
        let stats = r
            .policy_stats
            .as_ref()
            .unwrap_or_else(|| panic!("playbooks report stats"));
        for s in stats {
            answered += s.get("playbook:answered").copied().unwrap_or(0);
            assert_eq!(s.get("playbook:errors"), None, "{s:?}");
        }
    }
    assert!(answered > 0);
    let _ = std::fs::remove_dir_all(dir);

    // A compile error stops the run before any game, with its line.
    let bad = std::env::temp_dir().join(format!("kiln-bad-{}.rhai", std::process::id()));
    std::fs::write(&bad, "fn priority(view, d) {\n  let = 3;\n}\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let spec = POD.replace(
        "default: { kind: greedy }",
        &format!("default: {{ kind: playbook, playbook: {} }}", bad.display()),
    );
    let err = play(&spec, &scratch("bad"), 1).expect_err("a broken playbook");
    assert_eq!(err.exit_code(), 2);
    assert!(err.to_string().contains(":2:"), "{err}");
    let _ = std::fs::remove_file(bad);
}

#[test]
fn playbook_check_reports_rules_and_compile_errors() {
    let Some((db, lib)) = library() else {
        return;
    };
    let root = repo_root();
    let deck = root.join("tests/fixtures/decks/najeela.txt");
    let check = experiment::check_playbook(
        &deck,
        &root.join("examples/playbooks/najeela.rhai"),
        4,
        db,
        lib,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(check.compile_error, None);
    assert_eq!(check.games, 4);
    assert!(check.hooks.contains(&"priority".to_owned()));
    assert!(
        check
            .counters
            .get("playbook:answered")
            .copied()
            .unwrap_or(0)
            > 0
    );
    assert!(check.counters.contains_key("tag:land drop"));
    assert!(check.errors.is_empty(), "{:?}", check.errors);

    let bad = std::env::temp_dir().join(format!("kiln-check-{}.rhai", std::process::id()));
    std::fs::write(&bad, "fn mulligan(view, d) {\n  return Keep\"\n}\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let check =
        experiment::check_playbook(&deck, &bad, 4, db, lib).unwrap_or_else(|e| panic!("{e}"));
    let err = check
        .compile_error
        .unwrap_or_else(|| panic!("a syntax error"));
    assert!(err.contains(":2:"), "{err}");
    let _ = std::fs::remove_file(bad);
}
