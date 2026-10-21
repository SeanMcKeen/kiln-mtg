---
type: strategy
status: draft
tags: [testing]
updated: 2026-10-20
---

# Testing Strategy

The rules engine has to be trustworthy, because every simulation result depends on it. Testing is therefore layered and **CR-traceable**.

| Layer | What | Where | Gate |
|---|---|---|---|
| Unit | Pure functions: mana algebra, filters, layer ordering, CR-ref parsing | each crate | PR |
| **CR-cited scenarios** | Board state → actions → expected state/events, tagged with CR rules | [[CR-Cited Scenario Tests]] | PR |
| **Card verification** | Per-card scenarios that promote a card to `verified` | `cards/tests/` | PR |
| **Golden replays** | Full games replayed, with the final hash asserted | [[Golden Replays]] | PR |
| **Invariants / property tests** | Random states and actions never break invariants | [[Invariants & Property Tests]] | PR (short), nightly (long) |
| **Fuzzing via random play** | Millions of random-policy games: no panics, no invariant violations, deterministic replays | [[Fuzzing via Random Play]] | nightly |
| Benchmarks | Throughput and clone cost regressions | `benches/` | nightly; alerts |
| Agent evals | Realistic MCP/CLI tasks with Claude | [[MCP Server Tool Catalog]] | per release |
| Play-quality suite | Puzzle positions, policy ladder | [[Measuring Play Quality]] | per release |

## Principles
- **A test's expected outcome must be authoritative:** cite the CR rule or the Oracle ruling. ⚠-confidence entries in [[Tricky Interactions Catalog]] are verified before blessing.
- **Scenario tests are data, not code**, so agents and humans can write them ([[Scenario File Format]]).
- **Every bug fix adds a scenario** that reproduces it.
- **Coverage is reported per CR rule** (which rules have at least one test) and per card.

## Tooling candidates
`cargo-nextest`, `proptest`, `insta` (snapshot tests for event logs), `criterion`, `cargo-fuzz` (libFuzzer) for parser fuzzing.
