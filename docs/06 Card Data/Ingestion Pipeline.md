---
type: design
status: draft
tags: [card-data, pipeline]
updated: 2026-10-22
---

# Ingestion Pipeline

```
fetch (Scryfall bulk index → oracle_cards.jsonl.gz, rulings, oracle_tags)
  → verify (size, updated_at; MTGJSON SHA-256 when used)
  → normalize (faces, layouts, symbols, self-name → ~, strip reminder text into separate field)
  → card records (Card Data Model)
  → card-definition front-ends (parser / DSL overrides / escapes) → Card IR
  → coverage computation (unsupported / parsed / verified + provenance)
  → write versioned binary card DB  (data/cache/carddb-<date>-<hash>.bin)
  → write coverage report (JSON + markdown summary)
```

## Requirements
- **Reproducible:** the card DB version = `(source, source date, pipeline version, overrides hash)`. It's stamped into every report (NFR-D2).
- **Offline-capable:** once cached, the pipeline runs without network access. The sim never fetches at runtime unless explicitly asked.
- **Polite:** use bulk files, never per-card API calls for ingestion. Send a descriptive User-Agent. Cache ≥ 24 h, per Scryfall guidance.
- **Robust to new layouts:** unknown `layout` values (e.g. `prepare` and `front_card` appeared recently) produce `unsupported` records with a reason. They never crash the pipeline.
- **Names aren't unique.** In the 2026-10-20 `oracle_cards` file, **40 names map to more than one Oracle object**, excluding art-series and tokens. Examples: playtest and "mystery booster" variants (`Inferno`, `Red Herring`, `Savage Lands`), Un-cards, and a non-legal `Llanowar Elves` variant. Name → Oracle ID resolution must prefer:
  1. legal in the target format
  2. `games` includes `paper`
  3. non-playtest set types

  Report ambiguity when it remains ([[Deck Import]]).
- **Diffable:** each refresh produces a diff of new, removed, Oracle-text-changed (errata) and legality-changed cards. Errata demote `verified` cards to `parsed` ([[CR Update Process]]).
- **Fast load:** the binary DB loads in under 1 s (NFR-P6). Candidates are `rkyv` (zero-copy) or `bincode`.

## CLI (planned)
```
kiln data fetch [--source scryfall|mtgjson] [--date latest]
kiln data build [--overrides cards/overrides/]
kiln data coverage [--format commander] [--deck path]
kiln data diff <old-db> <new-db>
```

## Related
[[Card Data Sources]] · [[Card Data Model]] · [[M1 Foundations & Data]]
