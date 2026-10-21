---
type: research
status: draft
tags: [card-data, research]
updated: 2026-10-20
---

# Card Data Sources

## Candidates
| | **Scryfall** | **MTGJSON** |
|---|---|---|
| What | Card search site with a public API and **daily bulk exports** | Open-source project publishing card data as JSON, CSV, SQL and other files |
| License / terms | Free for Magic software, research and community content under the WotC Fan Content Policy. Cache data ≥ 24 h. Overloading the API risks an IP ban. | MIT License (per the MTGJSON FAQ). Card *content* is still WotC IP under the Fan Content Policy. |
| Oracle-level file | `oracle_cards`: one object per Oracle ID. ~25 MB gzipped JSONL on 2026-10-20 | `AtomicCards.json`: Oracle-style data per card, with no printing-specific fields |
| Printings | `default_cards` (~80 MB gz), `all_cards` (~400 MB gz, all languages) | `AllPrintings.json` |
| Rulings | **`rulings` bulk file, keyed by `oracle_id`** (~5 MB gz) | Rulings embedded per card |
| Tags | **`oracle_tags`**: community Tagger tags (e.g. functional categories) | — |
| Identifiers | `oracle_id`, `id` (printing), cross-IDs (MTGO, Arena, TCGplayer, Cardmarket) | `uuid`, plus `identifiers.scryfallOracleId`, `scryfallId`, … |
| Integrity | `updated_at` per bulk file | SHA-256 checksum per file, `Meta.json` version (`5.3.0+20261020` on 2026-10-20) |
| Format support | `legalities` per format, **`game_changer`** flag (Commander brackets), `edhrec_rank`, `keywords`, `color_identity`, `produced_mana` | `legalities`, `colorIdentity`, `keywords`, `leadershipSkills` |
| Access notes | Bulk index at `https://api.scryfall.com/bulk-data`. Each entry has a `jsonl_download_uri` and `compressed_size`. Requests should send a descriptive `User-Agent` and an `Accept` header. | Static file downloads from mtgjson.com |

## Observed fields on Scryfall `oracle_cards` objects (2026-10-20)
`oracle_id, name, layout, mana_cost, cmc, type_line, oracle_text, power, toughness, colors, color_identity, keywords, legalities, game_changer, reserved, card_faces (for multi-face), all_parts, edhrec_rank, games, set, collector_number, rulings_uri, …`

## Recommendation (→ [[ADR-0009 Card Data Source of Truth]])
- **Primary rules data: Scryfall `oracle_cards` + `rulings` (+ `oracle_tags`)**, keyed by **Oracle ID**. Why:
  - Oracle-level granularity is exactly the rules identity
  - rulings are keyed the same way, which feeds test-case generation
  - Tagger tags give free functional categories for per-card analytics ("tutor", "ramp", "counterspell")
  - `game_changer` supports Commander bracket analysis
- **Secondary: MTGJSON** as a cross-check and fallback. It is MIT-licensed and has checksums. Use it for printings and set metadata if ever needed.
- **Both are fetched at build or run time and cached under `data/cache/`, never committed** ([[Legal & Licensing]]).

## Sources
- Scryfall bulk data index (queried 2026-10-20): https://api.scryfall.com/bulk-data
- Scryfall API docs: https://scryfall.com/docs/api and https://scryfall.com/docs/api/bulk-data
- MTGJSON FAQ (MIT license, AtomicCards description, SHA-256 checksums): https://mtgjson.com/faq/
- MTGJSON identifiers data model: https://mtgjson.com/data-models/identifiers/
- MTGJSON Meta (queried 2026-10-20): https://mtgjson.com/api/v5/Meta.json
