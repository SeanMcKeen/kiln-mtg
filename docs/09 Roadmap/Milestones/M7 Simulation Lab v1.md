---
type: milestone
status: review
milestone: M7
tags: [milestone, simulation]
updated: 2026-10-27
---

# M7: Simulation Lab v1

## Checklist
- [x] `kiln-protocol` with JSON Schemas: spec, report, replay ([[ADR-0011 Experiment Spec and Report Schema]])
- [x] Experiment spec parsing and validation ([[Experiment Spec]])
- [x] Batch runner: rayon, deterministic seeds, checkpointing, progress, cancellation ([[Batch Runner & Parallelism]])
- [x] Metrics with flags: win_rates, game_shape, mulligans, per_card, interaction, mana, timing, decision_stats, replays, raw_events ([[Metrics Catalog]])
- [x] Statistics: Wilson and bootstrap CIs, seat analysis, BH correction ([[Statistics for Multiplayer Simulation]])
- [x] Reports: concise and detailed JSON, markdown, JSONL/CSV tables (Parquet deferred: Q38) ([[Output Formats & Report Schema]])
- [x] Modes: pod, goldfish, ab, opening_hands
- [x] CLI: `kiln sim run|status|report|compare`, `kiln game replay|inspect` ([[CLI & REPL]])
- [x] Validity warnings ([[Validity & Caveats]])

## Exit criteria
US-2 and US-4 work end to end. US-1 runs and produces a report with correct CIs and validity warnings. 5,000 greedy games finish in under 1 minute on 8 cores.

**Met (2026-10-27).**
- **US-1** (`examples/specs/us1-kinnan-pod.yaml`): 5,000 four-player games of the M6 test decks with balanced seating. Every win rate has a Wilson interval (about ±1.3 pp); the concise report is 1.8 KB and carries the baseline-pilot warning. Example: Kinnan 28.5% [27.2, 29.7], Najeela 40.0% [38.6, 41.3].
- **US-2** (`examples/specs/najeela-goldfish.yaml`): goldfish against three passive opponents; Najeela's median win is its own turn 10 [10, 11].
- **US-4** (`examples/specs/kinnan-opening-hands.yaml`): 2,000 opening hands: kept at 7 in 60.3% of hands, with fast mana in 31.1% of kept hands.
- **Throughput:** 5,000 greedy games in **31.8 s on 2 threads** (≈157 games/s); 8 cores would be well under a minute. Not run on 8 cores, to keep the laptop cool.
- Determinism: records and reports are byte-identical on 1 and 2 threads, resumed runs match uninterrupted ones, and kept replays are re-played and checked after every run.
- Observation: greedy pilots show **no first-seat advantage** (seat 1 24.2% vs seat 4 25.6%), unlike real cEDH. Their games run long enough to wash out turn order. This is a play-quality signal for M9 (Q39).

## Starting point (2026-10-27)
- `kiln-sim::runner` runs batches with rayon and deterministic per-game seeds, decklist seats (`SeatDeck`), random/greedy/mixed policies, invariants, view audits and replay checks. It reports counts only (wins by seat, endings).
- `kiln-protocol` is an empty stub. There's no spec, report, statistics, run directory or `kiln sim` command.
- Policies available: random, greedy, passive, scripted. Playbook (M8) and search (M9) policies don't exist yet.

## Plan
### Design decisions
| # | Decision | Why |
|---|---|---|
| S1 | **`kiln-protocol` owns the wire types**: `kiln-spec/1` (experiment spec) and `kiln-report/1` (report, game record). Serde with `deny_unknown_fields`; JSON Schemas generated with `schemars`, committed under `crates/kiln-protocol/schemas/`, and a test fails if they drift. | ADR-0011: the spec is the contract for the CLI, MCP, Python and agents. |
| S2 | **Unknown keys are errors with a suggestion**: serde's "unknown field `x`, expected one of …" is rewritten with the closest name (Jaro–Winkler, `strsim`). | Agent-friendly errors (Experiment Spec validation rules). |
| S3 | **Decks**: `file`, `text` and `moxfield_export` (pasted text) sources, through the M1 deck parser and validator. The coverage gate `require_verified` (default) / `warn` / `allow`; a failing gate exits with code 3. URLs wait for M8 (network fetch with caching). | FR-S8; offline runs. |
| S4 | **Seating**: `fixed`, `random` (seeded per game) and `balanced_rotation` (the N cyclic rotations, default) or `all_permutations`. `games` is rounded up to a multiple of the rotation count, with a warning. | Seat bias is large in cEDH (Statistics for Multiplayer Simulation). |
| S5 | **Policies** per deck (a `default` plus per-deck entries), following the deck across seats. Kinds now: `random { pass }`, `greedy`, `passive`. `playbook`, `search` and `llm` are rejected with the milestone that brings them. A validity warning when decks have different policy kinds. | Asymmetric pilots bias results (Validity & Caveats). |
| S6 | **Limits**: `max_turns` (the engine's turn limit) and `max_decisions` (the harness cap) end a game as **capped**: excluded from win-rate denominators, counted and warned about. `max_wall_ms` is off by default; if set, results can depend on hardware, so the report warns. | ADR-0005: reports are bit-identical for the same inputs. |
| S7 | **Per-game records are the source of truth.** Each finished game becomes one `GameRecord` (seating, seeds, result, finishing order, win type, turns, mulligans, per-card facts when enabled). Every report section is computed from records sorted by game index, so `kiln sim report` can rebuild it and float sums are in a fixed order. | Bit-identical reports; resume. |
| S8 | **Run directory** `runs/<id>/`: `spec.resolved.yaml` (defaults filled, hash in the report), `games.jsonl` (checkpoint, appended as games finish), `progress.jsonl`, `report.json`, `report.md`, `cards.jsonl`, `replays/`. **Resume** skips game indices already in `games.jsonl`. **Cancellation** is a cooperative flag checked between games and decisions; the CLI's Ctrl-C relies on resume. | Batch Runner & Parallelism. |
| S9 | **Metrics are flags.** Categories that need the event log (`per_card`, `interaction`, `mana`, `decision_stats`, `raw_events`) turn event logging on; the rest cost nothing. `timing` uses wall-clock time, so it goes in a section labelled non-deterministic and is left out of the report hash. | Metrics Catalog: "off means zero cost". |
| S10 | **Statistics**: Wilson 95% intervals for every rate; seeded percentile bootstrap for means and medians; Benjamini–Hochberg for per-card rankings; paired differences for A/B (common random numbers) with a paired bootstrap. | Statistics for Multiplayer Simulation. |
| S11 | **Win types** from the engine's endings: `alt_win` (an effect win), `life`, `commander_damage`, `decking`, `poison`, `effect_loss` (players eliminated by an effect), `concession`, `last_standing`, `draw`. | Metrics Catalog `game_shape`. |
| S12 | **Tables are JSONL and CSV; Parquet is deferred.** The `arrow`/`parquet` crates add hundreds of dependencies and long compiles. JSONL loads directly into DuckDB, polars and pandas, and DuckDB converts it to Parquet in one statement. Open question Q38. | Cost vs. benefit at M7. |
| S13 | **Modes**: `pod` (N decks), `goldfish` (one deck vs. passive opponents with basic-land decks; the win turn is the turn the last opponent leaves), `opening_hands` (deal and mulligan only, no game) and `ab` (variants with card swaps, paired seeds). | US-1, US-2, US-4. |
| S14 | **CLI**: `kiln sim run|status|report|compare` and `kiln game replay|inspect`, with `--json` everywhere and the exit codes from CLI & REPL (2 invalid input, 3 coverage gate, 4 run failed, 5 determinism divergence). | CLI & REPL. |

### Phases
| Phase | Content | Evidence |
|---|---|---|
| 7.1 | `kiln-protocol` spec and report types, JSON Schema export, spec parsing with suggestions, validation, deck resolution and the coverage gate. | Unit tests; schema drift test |
| 7.2 | Experiment runner: `pod` and `goldfish`, seating, policies, limits, game records, run directory, checkpoint and resume, progress. | Determinism across thread counts; resume test |
| 7.3 | Statistics and reports: `win_rates`, `game_shape`, `mulligans`, `timing`, `decision_stats`; Wilson, bootstrap, BH; concise, detailed and markdown reports; validity warnings. | Statistics unit tests; report snapshot |
| 7.4 | `per_card`, `interaction`, `mana`, `replays`, `raw_events`; `opening_hands` and `ab` modes. | US-4; A/B paired test |
| 7.5 | CLI `kiln sim …` and `kiln game …`; US-1, US-2 and US-4 end to end; throughput check; docs; ADR-0011 review → merge. | Exit criteria |

## Progress
| Phase | Status | Evidence |
|---|---|---|
| 7.1 | ✅ 2026-10-27 | `kiln-protocol`: `kiln-spec/1` types with defaults and `deny_unknown_fields`, YAML/JSON parsing with "did you mean" suggestions, validation listing every problem, balanced-seating rounding, the resolved-spec hash. JSON Schemas generated with `schemars` and committed under `crates/kiln-protocol/schemas/` with a drift test. Deck sources, legality, the coverage gate (exit code 3) and A/B variants by card swaps. |
| 7.2 | ✅ 2026-10-27 | Experiment runner: decks mapped onto the card library's pool, per-deck policies, configurations (pod, goldfish, A/B), seating (fixed, random, balanced rotation, all permutations; seat 1 always plays first), limits as capped games. `GameRecord`s per game; the run directory with `run.json`, `games.jsonl` (checkpoint), `progress.jsonl`; resume; cooperative cancellation. |
| 7.3 | ✅ 2026-10-27 | Statistics (Wilson, nearest-rank quantiles, seeded bootstrap medians, paired bootstrap, Benjamini–Hochberg, two-proportion test) and `kiln-report/1`: win rates by deck and seat, head-to-head, finishing positions, game length and rounds, win turn and win types, mulligans, decision stats, timing (non-deterministic, labelled). Validity warnings, notable findings, follow-up hints; concise JSON, detailed JSON, markdown, `games.csv`. |
| 7.4 | ✅ 2026-10-27 | Event-log facts (only when a metric needs them): per-card seen/cast turns, interaction, mana. Per-card report (GIH/OH/cast win rates, IWD with BH q-values, labelled correlational). A/B comparisons with paired-bootstrap intervals. `opening_hands` mode. Replays kept by re-playing selected games after the run (a determinism check: exit code 5). `raw_events`. Win and loss turns count the deck's own turns. |
| 7.5 | ✅ 2026-10-27 | CLI `kiln sim run|status|report|compare` and `kiln game replay|inspect`; the run records the spec's directory so games can be re-played later. Example specs in `examples/specs/`. US-1, US-2 and US-4 end to end; throughput; docs; ADR-0011 accepted. |
