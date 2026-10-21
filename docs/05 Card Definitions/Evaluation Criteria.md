---
type: criteria
status: draft
tags: [card-definitions]
updated: 2026-10-20
---

# Evaluation Criteria: Card Definition Approach

Score each option from 1 to 5 against the spike results ([[Card Definition Spike Plan]]). Weights reflect the headline use case.

| # | Criterion | Weight | How measured |
|---|---|---|---|
| 1 | **Correctness / verifiability** | 25% | % of the cEDH pool passing scenario tests. Rate of silent mis-parses or mis-authorings found by review. |
| 2 | **Velocity to the full pool** | 20% | Cards per engineer-day (or per agent-hour) reaching `verified`. Projected time to 32k. |
| 3 | **Maintenance under errata and CR changes** | 15% | Effort to absorb one set release and one CR update. |
| 4 | **Runtime performance** | 10% | IR interpretation cost in the sim hot loop. Should be equal across options if all compile to the same IR. |
| 5 | **AI-authorability** | 10% | How reliably an LLM produces correct definitions or overrides from Oracle text, judged by test pass rate. |
| 6 | **Expressiveness** | 10% | Fraction of the 20 hardest cEDH cards expressible without the escape hatch. |
| 7 | **Upfront cost** | 10% | Engineering time before the first 100 cards work. |

## Hard requirements (pass/fail)
- Must compile to the shared typed IR.
- Must support per-card coverage status and provenance.
- Must never silently produce a "blank" card. Unsupported text must be detected.
