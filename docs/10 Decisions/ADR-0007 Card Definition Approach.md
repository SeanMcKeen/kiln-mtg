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
*Pending spike results, scored with [[Evaluation Criteria]].*
