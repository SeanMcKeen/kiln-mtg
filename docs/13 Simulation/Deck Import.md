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

## Validation as built (M1)
`mtg deck check <file> [--commander NAME]… [--json]` (in `mtg-sim::deck`) runs these checks, each tagged with its rule:

| Check | Rule |
|---|---|
| Commander designated: a `Commander` section, `*CMDR*`, an Archidekt `[Commander]` tag, or `--commander` (added to the list if missing) | CR 903.3 |
| Eligibility: a legendary creature, Vehicle, or Spacecraft with P/T, or "can be your commander" | CR 903.3, 903.3a |
| Two commanders need a shared partner ability. Variants don't mix | CR 702.124h–m, 702.124f |
| A Background is only legal with "choose a Background" | CR 702.124k |
| Exactly 100 cards | CR 903.5a |
| Singleton, except basics, "any number of cards named ~" and "up to N cards named ~" | CR 903.5b |
| Color identity ⊆ the commanders' combined identity | CR 903.5c, 702.124c |
| Basic land types must produce colors inside the identity | CR 903.5d |
| Sideboard, maybeboard and companion sections are ignored with a warning | CR 903.5e |
| Banned and not-legal cards, from card DB legality data (not a CR rule) | — |

- **Color identity used:** the union of Scryfall's value and the CR 903.4 recomputation ([[Ingestion Pipeline]]). Each catches cases the other misses, and the union never admits an off-identity card.
- **Name matching:**
  - Exact after normalization (case, accents, curly apostrophes).
  - Punctuation-insensitive matches are accepted with a warning ("Thassas Oracle").
  - Anything fuzzier is an error with the top 3 suggestions, never a silent pick.
- **Output:** concise JSON with `--json`. The coverage summary includes `experiment_ready`, which stays false until every card is verified (FR-S8). Exit code 0 means legal, 1 illegal, 2 an error.

## Output
`DeckRecord { id, name, commanders[], cards: [(oracle_id, qty)], hash, warnings[] }`. The `hash` (sorted Oracle IDs and quantities) identifies the deck in reports and replays.

## Sources
- Moxfield/Archidekt export conventions, as described by third-party importers: https://scrytics.com/blog/mtg-collection-import-export, https://mythichub.com/help/importing-decklists

## Related
[[Experiment Spec]] · [[Card Data Model]] · [[MCP Server Tool Catalog]]
