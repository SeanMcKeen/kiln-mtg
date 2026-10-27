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

## As built (M1)
- `kiln data fetch [--rulings] [--force]` downloads Scryfall bulk files into `data/cache/`, with a `.meta.json` sidecar holding `updated_at`, size and `fetched_at`. A copy younger than 24 h is reused unless `--force` is given.
- `kiln data build [--input FILE] [--source-date D]` writes `carddb-<date>-<hash8>.bin`, a `current` pointer, and a JSON and markdown build report. The DB id is `scryfall-<date>#<hash8>`, where the hash is the SHA-256 of the payload.
- **Name index (two tiers).** Full names and the `A/B` spelling of `A // B` always win over face names. Face names are indexed for split, flip, transform and MDFC cards. For adventure and prepare cards only the front face is indexed, because Secrets of Strixhaven prepare cards have spell faces named "Brainstorm", "Ancestral Recall" and "Channel". The report lists **shadowed face names**: 11 in the 2026-10-20 data.
- **Duplicate full names:** 16 among deck-eligible objects in the 2026-10-20 data. The earlier count of 40 used a different normalization. All 16 either resolve through the legality → paper → non-joke tie-break or are never legal (Un-card variants). Banned cards count as "has a legality", so they still resolve and can be reported as banned.
- **Color identity cross-check:** 134 differences between the source and the CR 903.4 recomputation. Most are lands with basic land types, which Scryfall colors and strict CR 903.4/903.4c leave colorless; CR 903.5d covers them in deckbuilding. The rest are preview cards with incomplete data, meld results and Un-cards. See [[Deck Import]] for how validation combines the two.
- **Data issues:** 4 cards have mana symbols the engine doesn't model (`{L}`, `{D}`, `{C/P}`). They're recorded as unsupported reasons.

## CLI (planned)
```
kiln data fetch [--source scryfall|mtgjson] [--date latest]
kiln data build [--overrides cards/overrides/]
kiln data coverage [--format commander] [--deck path]
kiln data diff <old-db> <new-db>
```

## Related
[[Card Data Sources]] · [[Card Data Model]] · [[M1 Foundations & Data]]
