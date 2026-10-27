---
type: milestone
status: in-progress
milestone: M7
tags: [milestone, simulation]
updated: 2026-10-27
---

# M7: Simulation Lab v1

## Checklist
- [ ] `kiln-protocol` with JSON Schemas: spec, report, replay ([[ADR-0011 Experiment Spec and Report Schema]])
- [ ] Experiment spec parsing and validation ([[Experiment Spec]])
- [ ] Batch runner: rayon, deterministic seeds, checkpointing, progress, cancellation ([[Batch Runner & Parallelism]])
- [ ] Metrics with flags: win_rates, game_shape, mulligans, per_card, interaction, mana, timing, decision_stats, replays, raw_events ([[Metrics Catalog]])
- [ ] Statistics: Wilson and bootstrap CIs, seat analysis, BH correction ([[Statistics for Multiplayer Simulation]])
- [ ] Reports: concise and detailed JSON, markdown, Parquet tables ([[Output Formats & Report Schema]])
- [ ] Modes: pod, goldfish, ab, opening_hands
- [ ] CLI: `kiln sim run|status|report|compare`, `kiln game replay|inspect` ([[CLI & REPL]])
- [ ] Validity warnings ([[Validity & Caveats]])

## Exit criteria
US-2 and US-4 work end to end. US-1 runs and produces a report with correct CIs and validity warnings. 5,000 greedy games finish in under 1 minute on 8 cores.

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
