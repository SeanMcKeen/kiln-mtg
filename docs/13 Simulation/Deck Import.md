---
type: design
status: draft
tags: [simulation, decks, import]
updated: 2026-10-20
---

# Deck Import

## Supported inputs (priority order)
1. **Plain text (MTGO/Arena-style):** lines of `qty Card Name`, optionally `(SET) number`. Section headers like `Commander`, `Deck`, `Sideboard`, `SIDEBOARD:`, `Companion`. This is the universal export format: Moxfield and Archidekt both export text.
2. **Moxfield / Archidekt text exports.** Moxfield's text export may not mark commanders, so commander designation needs either a `Commander` section, a `*CMDR*` marker, or an explicit `commanders:` field in the spec. Archidekt text includes category headers, which are ignored except "Commander".
3. **CSV** (Archidekt/Deckbox-style columns).
4. **URLs:** *only if* a documented public API exists. No official Moxfield or Archidekt API docs were found during research, and scraping may violate their terms, so users paste exports instead ([[Legal & Licensing]]).

## Resolution
- Names resolve to Oracle IDs with **fuzzy matching and suggestions** ("Thassas Oracle" → "Thassa's Oracle"). Split, DFC and adventure names accept either face name or the full `A // B` form.
- **Duplicate names exist** (40 in the 2026-10-20 data). Resolution prefers the Oracle object that is legal in the spec's format and playable in paper ([[Ingestion Pipeline]]).
- Printing info is ignored. Rules identity is Oracle-level.
- Validation output:
  - format legality, with **banned cards listed**
  - commander color identity violations
  - singleton violations
  - deck size
  - **coverage status of every card**, as the gate for experiments

## Output
`DeckRecord { id, name, commanders[], cards: [(oracle_id, qty)], hash, warnings[] }`. The `hash` (sorted Oracle IDs and quantities) identifies the deck in reports and replays.

## Sources
- Moxfield/Archidekt export conventions, as described by third-party importers: https://scrytics.com/blog/mtg-collection-import-export, https://mythichub.com/help/importing-decklists

## Related
[[Experiment Spec]] · [[Card Data Model]] · [[MCP Server Tool Catalog]]
