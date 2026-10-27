---
type: adr
status: accepted
tags: [adr, card-data]
updated: 2026-10-21
---

# ADR-0009: Card Data Source of Truth

## Decision
- **Primary source:** Scryfall bulk `oracle_cards`, plus `rulings` and `oracle_tags`. **Oracle ID** is the primary card key.
- **Secondary:** MTGJSON (MIT, checksummed) for cross-checks and printings.
- Name resolution handles duplicate names (40 cases in the 2026-10-20 data) by format legality and paper availability.

See [[Card Data Sources]] and [[Ingestion Pipeline]].

## Rationale
- Oracle-level granularity.
- Rulings keyed by Oracle ID feed tests.
- Tagger tags support per-card analytics categories.
- The `game_changer` flag supports Commander brackets.
- JSONL bulk downloads are easy to stream.

**Accept by:** M1.

## M1 evidence (2026-10-20)
`kiln-data` now ingests Scryfall `oracle_cards` end to end (`kiln data fetch|build`), and the evidence supports the proposal:
- **Oracle ID works as the key.** 38,940 objects, all with an Oracle ID. The DB is content-hashed and rebuilds byte-identically.
- **Legality data is enough for deck validation.** The four Commander bans (Mana Crypt, Dockside Extortionist, Jeweled Lotus, Hullbreacher) are rejected from data alone, with nothing hard-coded.
- **Name resolution needs care beyond what this ADR says.** 16 full names map to several deck-eligible objects (mostly Un-card variants). Separately, prepare-card spell faces reuse real card names ("Brainstorm"). A legality → paper → non-joke tie-break plus a two-tier name index handles both ([[Ingestion Pipeline]]).
- **Scryfall's `color_identity` isn't a pure CR 903.4 value.** It includes basic-land-type colors and is empty for some preview cards whose rules text has mana symbols. Validation uses the union with a recomputed value. That's a small caveat, not a reason to switch sources.
- **MTGJSON cross-checks haven't been built yet.** They aren't needed for M1's exit criterion.

**Status: accepted (2026-10-21)** by the user, on this evidence.
