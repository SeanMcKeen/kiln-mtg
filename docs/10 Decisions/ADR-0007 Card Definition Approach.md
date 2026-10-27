---
type: adr
status: accepted
tags: [adr, card-definitions]
updated: 2026-10-21
---

# ADR-0007: Card Definition Approach

## Status
**Accepted (2026-10-21): Option D, hybrid.** The user asked for this to be researched rather than decided up front, then approved the researched recommendation below.

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
**Option D, hybrid IR**, with the shape listed under the recommendation: a compositional Oracle-text parser first, strict YAML overrides for the long tail, named Rust escapes only for truly unique cards, a mandatory renderer lint, and CR-cited scenarios for `verified`. Data and scoring are in [[Card Definition Spike Results]].

Precedence when sources disagree: a YAML override replaces the parser's output for that card; an escape replaces only the ability it names. A card the parser can't fully cover, and that has no override, stays `unsupported`. It's never partly parsed.

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
