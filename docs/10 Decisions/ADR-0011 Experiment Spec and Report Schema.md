---
type: adr
status: proposed
tags: [adr, simulation, schema]
updated: 2026-10-22
---

# ADR-0011: Experiment Spec and Report Schema

## Decision (proposed)
- Declarative, versioned specs (`kiln-spec/1`, YAML or JSON) cover decks, modes, seating, per-deck or per-seat policies, limits, the coverage gate, **per-category metric flags**, and outputs ([[Experiment Spec]]).
- Reports (`kiln-report/1`) come in two forms. The concise one is ≈ ≤ 2k tokens and agent-first, with CIs on every rate, validity warnings and `next` hints. The detailed one is paginated, with Parquet/JSONL tables ([[Output Formats & Report Schema]]).
- Replays use the `kiln-replay/1` format.
- Unknown spec keys are errors, with suggestions.

## Consequences
The spec is the contract between the CLI, MCP, Python and agents. Schemas are exported and versioned.

**Accept by:** M7.
