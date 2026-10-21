---
type: design
status: draft
tags: [simulation, performance, parallelism]
updated: 2026-10-20
---

# Batch Runner & Parallelism

## Model
- **One game = one task.** Use the `rayon` thread pool, or a custom work-stealing scheduler if per-game durations vary wildly, which they will with search AI.
- **The card DB is shared** read-only via `Arc`. Policies are instantiated per game (or per worker, with reset) to avoid shared mutable state.
- **Seeds:** `seed_i = splitmix64(experiment_seed ⊕ hash(variant, i))`. The result for game *i* is independent of which thread runs it or when ([[Determinism & Replay]]).
- **Aggregation:** order-independent (counts and sums), or a sort by game index at the end. Floating-point sums are done in a fixed order (by game index) for bit-identical reports.

## Long runs
- **Progress events:** games done, ETA (EWMA of per-game time), interim win rates.
- **Checkpointing:** per-game records are appended to `runs/<id>/games.jsonl` as they complete. A run can resume after a crash by skipping completed indices.
- **Cancellation:** a cooperative flag checked between decisions.
- **Detach mode:** the CLI `--detach` and the MCP async pattern ([[MCP Server Tool Catalog]]).

## Throughput targets
See [[Non-Functional Requirements]]. Engine overhead is about 5 ms or less per cEDH game, so policy cost dominates. The runner should report **time spent per policy kind**, which helps users pick budgets.

## Early stopping (optional, later)
Sequential testing: stop when the CI half-width of the primary metric falls below a target (e.g. ±1.5 pp). Must be pre-declared in the spec to avoid peeking bias ([[Statistics for Multiplayer Simulation]]).

## Related
[[Simulation Harness]] · [[Reproducibility]]
