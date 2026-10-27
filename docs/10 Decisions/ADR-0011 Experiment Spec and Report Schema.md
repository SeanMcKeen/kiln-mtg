---
type: adr
status: accepted
tags: [adr, simulation, schema]
updated: 2026-10-27
---

# ADR-0011: Experiment Spec and Report Schema

## Decision
- Declarative, versioned specs (`kiln-spec/1`, YAML or JSON) cover decks, modes, seating, per-deck or per-seat policies, limits, the coverage gate, **per-category metric flags**, and outputs ([[Experiment Spec]]).
- Reports (`kiln-report/1`) come in two forms. The concise one is ≈ ≤ 2k tokens and agent-first, with CIs on every rate, validity warnings and `next` hints. The detailed one is paginated, with Parquet/JSONL tables ([[Output Formats & Report Schema]]).
- Replays use the `kiln-replay/1` format.
- Unknown spec keys are errors, with suggestions.

## Consequences
The spec is the contract between the CLI, MCP, Python and agents. Schemas are exported and versioned.

**Accept by:** M7.

## M7 evidence (2026-10-27)
- `kiln-spec/1` is implemented in `kiln-protocol` (YAML or JSON). Unknown keys and values are errors that name the closest valid one; validation lists every problem. The resolved spec is stored with the run and hashed into the report.
- `kiln-report/1` has the concise form (the US-1 report is 1.8 KB) and the detailed form, both with 95% intervals on every rate, validity warnings and `next` hints. Bulk tables are JSONL and CSV; Parquet is deferred (open question Q38).
- Replays use `kiln-replay/1`. Kept replays are regenerated after each run and checked against the recorded games.
- JSON Schemas for the spec, game record, run manifest and both report forms are generated with `schemars` and committed under `crates/kiln-protocol/schemas/`, with a drift test.

**Status: accepted (2026-10-27)** on this evidence, under the user's standing approval of researched recommendations.
