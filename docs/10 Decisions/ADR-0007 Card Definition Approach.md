---
type: adr
status: open
tags: [adr, card-definitions]
updated: 2026-10-20
---

# ADR-0007: Card Definition Approach

## Status
**Open.** The user asked for this to be researched rather than decided up front. It's decided by the [[Card Definition Spike Plan]] in [[M3 Card Definitions v1]].

## Context
See [[Card Definitions Problem Statement]] and [[Oracle Text Corpus Analysis]]: ~32k Commander-legal cards, and 89% of distinct ability lines are unique, so composition is required.

## Options
- [[Option - DSL with Escape Hatch]]
- [[Option - Oracle Text Compiler]]
- [[Option - Code per Card]]
- [[Option - Hybrid IR]] (current leaning)
- [[Option - Build on phase.rs]]

## Fixed constraints (already decided)
- All options compile to **one typed IR** that the engine interprets.
- Per-card `provenance` and `coverage` (`unsupported` / `parsed` / `verified`).
- Never silently produce a blank card.

## Decision
*Pending: the user decides.* The spike is done; data and scoring are in [[Card Definition Spike Results]].

## Recommendation after the spike (2026-10-21)
**Option D, hybrid**, weighted score 4.10. The other options scored A 3.40, B 3.50, C 2.70 and E 3.10.

The evidence behind it:
- **The IR is expressive.** 40 cards, including 20 of the hardest cEDH cards, were written as strict YAML with zero Rust escapes (six bespoke primitives, noted).
- **The parser approach scales.** A ~900-line compositional prototype already covers 23% of all Oracle lines. Its biggest gaps are cheap keyword templates. phase.rs shows ~92% is reachable.
- **The renderer lint works.** It catches 95.7% of injected mis-parses and flags omitted abilities, so mis-parses don't pass silently.

The shape:
- parser first, porting phase.rs grammar coverage under ADR-0012 E3
- strict YAML overrides for the long tail
- Rust escapes only for the truly unique
- a mandatory renderer lint, and scenario tests for `verified`
- optionally, a time-boxed phase.rs importer that only ever produces `parsed`
