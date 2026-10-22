---
type: milestone
status: planned
milestone: M1
tags: [milestone]
updated: 2026-10-22
---

# M1: Foundations & Data

**Goal:** a working Rust workspace, core types, card data ingestion, deck import, a CR tooling base, and a decision on phase.rs.

## Checklist
- [ ] Install the Rust toolchain (rustup stable). Set up CI (fmt, clippy -D warnings, nextest)
- [ ] Create the workspace per [[Crate Layout]]: `kiln-types`, `kiln-data`, `kiln-testkit`, with stubs for the rest
- [ ] **phase.rs evaluation** (time-boxed, 1 week) → [[ADR-0012 Relationship to phase.rs]]
- [ ] CR parser: `cr-lf.txt` → rule tree, plus a citation validator (CI check for `CR x.y` in code and docs)
- [ ] `kiln-types` types: IDs, colors, mana symbols and costs, type line, CR refs
- [ ] `kiln-data`: Scryfall bulk fetch (polite UA, cache), normalization, `CardRecord`, versioned binary DB ([[Ingestion Pipeline]])
- [ ] Coverage model (`unsupported` for everything initially) ([[Card Data Model]])
- [ ] Deck import: text formats, fuzzy names, Commander validation (size, singleton, color identity, banlist) ([[Deck Import]])
- [ ] `kiln deck check` and `kiln data fetch|build|coverage` CLI
- [ ] Commit the corpus-analysis scripts as `tools/corpus-analysis`
- [ ] Decide ADR-0009 (data source of truth)

## Exit criteria
`kiln deck check kinnan.txt --json` reports legality and per-card coverage for a real cEDH list.
