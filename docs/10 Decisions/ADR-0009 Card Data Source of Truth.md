---
type: adr
status: proposed
tags: [adr, card-data]
updated: 2026-10-20
---

# ADR-0009: Card Data Source of Truth

## Decision (proposed)
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
