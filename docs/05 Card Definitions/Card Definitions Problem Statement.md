---
type: problem
status: draft
tags: [card-definitions]
updated: 2026-10-20
---

# Card Definitions: Problem Statement

## The question
**How does each of ~32,000 Commander-legal cards get rules-correct behaviour in the engine,** in a way that:
- scales to the full pool with sustainable effort
- is **verifiably correct**, card by card
- survives CR changes and Oracle errata
- runs fast in the simulation hot loop
- can be authored by humans **and AI agents**, which is a live option today and not available to Forge or XMage when they were designed

## Constraints from other decisions
- Rust ([[ADR-0001 Rust as Engine Language]]).
- Card behaviour must compile to a **typed IR the engine interprets** (see [[Architecture Overview]]), so the engine stays card-agnostic. All options target the same IR. The question is the *authoring front-end*.
- The coverage status workflow `unsupported → parsed → verified` ([[Card Data Model]]).
- The data is Oracle text from Scryfall/MTGJSON ([[Card Data Sources]]).

## What the corpus says
See [[Oracle Text Corpus Analysis]]. 89% of distinct ability lines are unique, but the phrase vocabulary is regular. Composition is mandatory.

## Options
| Option | Note |
|---|---|
| A. Declarative DSL + code escape hatch (Forge-like) | [[Option - DSL with Escape Hatch]] |
| B. Oracle-text compiler (Arena / phase.rs-like) | [[Option - Oracle Text Compiler]] |
| C. Code per card (XMage-like) | [[Option - Code per Card]] |
| D. Hybrid: parser-first, DSL override, code escape, all → one IR | [[Option - Hybrid IR]] |
| E. Build on or fork phase.rs's parser and IR | [[Option - Build on phase.rs]] |

Evaluation: [[Evaluation Criteria]]. Process: [[Card Definition Spike Plan]]. Decision: [[ADR-0007 Card Definition Approach]].

## Current leaning (not decided)
**D (Hybrid)**, informed by E. The leaning comes from:
- the corpus's regularity, which favours a parser
- the long tail, which needs overrides
- AI-assisted authoring, which favours a readable intermediate DSL that LLMs can write *and* that tests can verify

The decision is made after the spike, on real data.
