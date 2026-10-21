---
type: option
status: draft
tags: [card-definitions, option]
updated: 2026-10-20
---

# Option B: Oracle-Text Compiler

## What
Parse Oracle text directly into the IR with a compositional grammar (e.g. parser combinators like `nom` or `winnow`), with a small override table for cards the grammar can't handle.

**Prior art:**
- **MTG Arena's GRP** is a Python program that converts English rules text into CLIPS rules, handling "80% or so" of new cards automatically ([[Prior Art - MTG Arena GRE]]).
- **phase.rs** has a `nom`-based parser with classify → route → sub-parse → lower stages, covering ~34k cards with gaps ([[Prior Art - phase.rs]]).

## Pros
- Errata and new sets flow through automatically: fix the grammar once and thousands of cards benefit.
- Oracle text stays the single source of truth, so there is no drift.
- The best long-term scaling. It's where both Arena and the closest Rust analog landed.

## Cons
- A large upfront research and engineering project. The grammar *is* the product.
- **Silent mis-parses** are dangerous: plausible-looking but wrong IR. This needs strong verification (per-card tests and round-trip rendering of IR back to text for diffing).
- Hard edge cases include:
  - self-references with partial names ("Kiki-Jiki" vs. "Kiki-Jiki, Mirror Breaker")
  - pronoun antecedents ("it", "that player")
  - nested conditions
  - modal and level-up cards

## Mitigations
- **IR → canonical-text renderer:** render the IR back to templated English and diff it against the Oracle text. A mismatch marks the card unverified.
- Track grammar coverage per phrase.

## Cost estimate
Very high upfront, lowest marginal cost per card.
