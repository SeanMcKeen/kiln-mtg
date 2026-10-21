---
type: adr
status: accepted
decided: 2026-10-20
tags: [adr, legal]
updated: 2026-10-20
---

# ADR-0002: Licensing and Card Data Distribution

## Decision
- Code is licensed **MIT OR Apache-2.0** (the Rust ecosystem convention).
- **Card data, images and CR text are never committed or bundled.** They're fetched from Scryfall and/or MTGJSON (and WotC for the CR) at build or run time and cached locally (`data/cache/`, `docs/_external/`, both gitignored).
- The Fan Content Policy disclaimer goes in the README.
- Don't copy GPL code (Forge). MIT/Apache code (phase.rs, XMage) may be reused with attribution.

## Consequences
First-run setup requires a network fetch. The ingestion pipeline must be robust and cache-friendly. See [[Legal & Licensing]].
