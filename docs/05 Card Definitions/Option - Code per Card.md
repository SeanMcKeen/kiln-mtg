---
type: option
status: draft
tags: [card-definitions, option]
updated: 2026-10-20
---

# Option C: Code per Card

## What
Every card is a Rust module or function that builds its abilities from library helpers.

**Prior art:** XMage. Java classes per card, ~19k cards with "full rules enforcement" per its README ([[Prior Art - XMage]]).

## Pros
- Maximum expressiveness. Any card can be done.
- Strong typing catches many authoring errors at compile time.

## Cons
- ~32k Rust modules hurt compile times and repo size, and lead to duplicated logic.
- Errata need code edits. No single source of truth.
- Hard for non-programmers. LLMs can write it, but the review burden is high.

## Verdict
Recommended only as the **escape hatch** inside other options, not as the primary approach.
