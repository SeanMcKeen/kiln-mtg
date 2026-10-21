---
type: milestone
status: review
milestone: M1
tags: [milestone]
updated: 2026-10-20
---

# M1: Foundations & Data

**Goal:** a working Rust workspace, core types, card data ingestion, deck import, a CR tooling base, and a decision on phase.rs.

## Checklist
- [x] Install the Rust toolchain (rustup stable, pinned to 1.99.1 in `rust-toolchain.toml`). Set up CI (fmt, clippy -D warnings, nextest, CR citation check): `.github/workflows/ci.yml`
- [x] Create the workspace per [[Crate Layout]]: `mtg-core`, `mtg-data`, `mtg-testkit`, `mtg-cards` (coverage only), `mtg-sim` (deck import only), `mtg-cli`, plus stubs for the rest
- [x] **phase.rs evaluation** (time-boxed) → [[phase.rs Evaluation]]. Recommendation recorded in [[ADR-0012 Relationship to phase.rs]]. **The decision is the user's** (still open).
- [x] CR parser: `cr-lf.txt` → rule tree (3,298 rules plus the glossary), plus a citation validator (`mtg dev cr-check`, CI job). All 438 citations in code and docs are valid.
- [x] `mtg-core` types: IDs, colors, mana symbols and costs, type line, printed stats, CR refs
- [x] `mtg-data`: Scryfall bulk fetch (polite UA, 24 h cache), normalization, `CardRecord`, versioned binary DB ([[Ingestion Pipeline]])
- [x] Coverage model (`unsupported` for everything initially) ([[Card Data Model]])
- [x] Deck import: text and CSV formats, fuzzy names, Commander validation (size, singleton, color identity, basic land types, partners, banlist) ([[Deck Import]])
- [x] `mtg deck check` and `mtg data fetch|build|info|coverage` CLI
- [x] Commit the corpus-analysis scripts as `tools/corpus-analysis` (recreated; figures reproduced, see its README)
- [ ] Decide ADR-0009 (data source of truth). Evidence added; **ready for the user to accept**.

## Exit criteria
`mtg deck check kinnan.txt --json` reports legality and per-card coverage for a real cEDH list. ✅ Met on 2026-10-20: `tests/fixtures/decks/kinnan.txt` is LEGAL (100 cards, UG), and all 100 cards report `unsupported` with the reason "no card-definition front-end yet (M3)".

## Results (2026-10-20 Scryfall data)
| Measure | Value |
|---|---|
| Card DB build (38,940 objects) | 0.9 s |
| Card DB load | **~210 ms** (NFR-P6 target ≤ 1 s) |
| Deck-eligible cards | 34,786 |
| Commander-legal cards (format coverage view) | 32,199, all `unsupported` |
| Duplicate full names among deck cards | 16 (all resolved by the tie-break or never legal) |
| Color identity: computed vs Scryfall mismatches | 134 (see [[Ingestion Pipeline]]) |
| Rebuild determinism | Identical DB hash on rebuild and on shuffled input |
| Tests | 61 (nextest), including real-data tests that skip without local data |

## Findings that changed the design
- **Face names collide with real card names.** Secrets of Strixhaven `prepare` cards have spell faces named "Brainstorm", "Ancestral Recall" and "Channel". Name resolution is therefore two-tier: full names always win over face names, and adventure/prepare spell faces aren't indexed at all ([[Ingestion Pipeline]]).
- **Color identity uses the union of source and computed values** for validation. Scryfall gives basic-land-type lands their colors (strictly covered by CR 903.5d) and has incomplete identity for some preview cards. The union never accepts an off-identity card ([[Deck Import]]).
- **Banned lists aren't CR rules.** Ban violations cite no CR number. Their source is the card DB's legality data.
- **"Spell commander"** appears on 5 cards but isn't in the 2026-06-19 CR. It's handled through the CR 903.3a "can be your commander" text and logged in [[12 Open Questions]].

## Deferred (moved to later milestones)
- `mtg data diff` (errata and legality diffs): M3, when coverage demotion on errata matters.
- Scryfall `oracle_tags` ingestion: M7 (per-card analytics).
- MTGJSON cross-check: when ADR-0009 is accepted.
