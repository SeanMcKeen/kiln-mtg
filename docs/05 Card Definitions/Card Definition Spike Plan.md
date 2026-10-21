---
type: plan
status: draft
tags: [card-definitions, spike, plan]
updated: 2026-10-20
---

# Card Definition Spike Plan

**When:** the start of [[M3 Card Definitions v1]], after the M2 core loop exists. The IR draft begins in M2, because vanilla creatures and basic spells need *something*.
**Time box:** 2–3 weeks.
**Output:** [[ADR-0007 Card Definition Approach]] moves to *Accepted*.

## Inputs
- The [[Curated cEDH Core Pool]] (~200 cards) as the benchmark set.
- The phase.rs evaluation results ([[Option - Build on phase.rs]]).
- [[Oracle Text Corpus Analysis]] plus phrase-level follow-ups.

## Steps
1. **IR v0:** define the effect, filter, cost, trigger and static algebra needed for the core pool. Every core-pool card must be *expressible* in IR (hand-written in Rust for now).
2. **DSL prototype:** pick a syntax and author 40 cards (20 easy, 20 hard). Ask an LLM to author another 40 from Oracle text. Measure correctness with scenario tests.
3. **Parser prototype:** a `winnow`/`nom` grammar for the 40 most common phrase types. Measure the parse rate and correctness on the core pool and on a random 1,000-card sample.
4. **Renderer prototype:** IR → templated English, then diff against Oracle for the parsed cards. Measure how many mis-parses it catches.
5. **phase.rs comparison:** run their parser on the same 1,000-card sample, if feasible, and compare.
6. **Score** against [[Evaluation Criteria]] and write up the ADR.

## Exit criteria
- The ADR is accepted, with data.
- IR v1 is frozen enough for M3 card work.
- The plan to reach `verified` for the full core pool is costed.
