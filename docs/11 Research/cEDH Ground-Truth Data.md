---
type: research
status: draft
tags: [research, cedh, data, validation]
updated: 2026-10-20
---

# cEDH Ground-Truth Data

These are sources for (a) real competitive decklists to simulate and (b) real outcome data to sanity-check simulated results (US-8).

| Source | What | Access | Notes |
|---|---|---|---|
| **EDHTop16** (edhtop16.com) | cEDH tournament results, standings (W/L/D), commander meta share, conversion rates, card play rates, **entry decklists** | **Public GraphQL API at `https://edhtop16.com/api/graphql`**, no auth, introspection enabled, rate limits for fair use (per its `llms.txt`) | Sponsored by Topdeck.gg. Its open-source repo license wasn't stated, so check terms before bulk use. |
| **Topdeck.gg** | Tournament platform (pairings, results) | TBD | Likely the upstream source for EDHTop16. |
| **EDHREC** | Card popularity across Commander (casual-heavy), articles (e.g. seat-order analysis) | Website. Scryfall exposes `edhrec_rank` | Good for prioritizing coverage in [[M11 Full Card Pool]]. |
| **cedhstats.org / playgroup.gg** | Per-commander and per-seat results | Website | Small samples. |

## How we'd use it
1. **Decklists:** import top-converting lists for the meta's commanders as realistic test decks (M11).
2. **Validation targets:**
   - meta-level win and conversion rates per commander (directional only, since tournament play ≠ our pods)
   - seat-position effects
   - typical win turns
3. **Coverage prioritization:** card frequency across tournament decklists determines which cards to verify first.

## Caveats
Tournament results include human factors (skill, politics, time limits, draws by agreement) that simulations don't model. Treat agreement as weak evidence, and treat large disagreements as a prompt to investigate policies.

## Sources
- EDHTop16 about page: https://www.edhtop16.com/about
- EDHTop16 llms.txt: https://edhtop16.com/llms.txt
- EDHREC seat-order article: https://edhrec.com/articles/does-cedh-have-a-seat-order-problem
