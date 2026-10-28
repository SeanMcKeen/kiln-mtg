---
type: milestone
status: active
milestone: M11
tags: [milestone, cards]
updated: 2026-10-28
---

# M11: Full Card Pool

**Continuous from M6 onward.** This is the long tail.

## Checklist
- [ ] Coverage dashboard (per format, per set, per mechanic)
- [ ] Prioritize by cEDH relevance (EDHTop16 frequency), then Commander popularity (EDHREC rank), then everything else
- [ ] Agent-assisted override authoring pipeline (if ADR-0007 chooses a hybrid)
- [ ] Remaining mechanics: face-down, DFC variants, mutate/merge, day/night, monarch, initiative, dungeons, attractions (P2), restart and subgames

## Exit criteria
≥ 95% of cards seen in cEDH tournament decklists are verified. Overall Commander-legal coverage is published.

## Starting point (2026-10-28)
- 32,199 Commander-playable cards in the 2026-10-20 card DB: 283 verified, 2,791 parsed (run from the Oracle-text parser without a scenario), 29,125 unsupported, nearly all for "no parser template".
- cEDH relevance data: EDHTop16's GraphQL API now accepts only the site's persisted queries ("Plain text queries are disabled"), and its current source repository isn't public, so card play rates can't be fetched as the research note planned ([[cEDH Ground-Truth Data]]). Lifting query ids from the site's JavaScript would work around a restriction they chose; not done.

## Plan
### Design decisions
| # | Decision | Why |
|---|---|---|
| C1 | **Dashboard**: `kiln data coverage --dashboard [--decklists DIR] [--next N] [--out FILE]` reports verified / parsed / unsupported for all playable cards (banned cards left out), Game Changers, EDHREC top 100/500/1000/5000, the 30 most common keywords, a decklist corpus (distinct cards and appearances), the top unsupported reasons, and the next cards to verify. Published as `docs/06 Card Data/Coverage Report.md` (names and counts only, no Oracle text). | Per mechanic and per priority slice; the card records carry no set code, so per-set waits for a data change. |
| C2 | **Priority**: appearances in the decklist corpus, then Game Changers, then EDHREC rank. The corpus is any folder of decklists: the test decks now, real tournament lists when available. | Tournament data is the exit criterion's measure; the proxies order work until it arrives. |
| C3 | **Exit measure, for now**: verified share of **appearances** in the corpus (each copy in each list), plus Game Changers and EDHREC top 100 as proxies. When a tournament corpus exists (EDHTop16 access, or exported lists), the same command measures the real criterion. | The criterion as written needs data we can't fetch (Q43). |
| C4 | **Authoring loop** (the agent-assisted pipeline, ADR-0007): for each card in priority order, a parsed card needs only a rules scenario (`verifies:`); an unsupported card needs an IR override in `cards/defs/` plus a scenario, and engine work when the IR lacks a feature (`support.rs` names it). Batches end with `kiln dev verify-cards` and the dashboard. | The same loop that built the 278 verified cards. |

### Phases
| Phase | Scope |
|---|---|
| 11.1 | Dashboard, priority queue, published report |
| 11.2 | Batch 1: the 18 unsupported cards of `tests/fixtures/decks/kinnan.txt` (the original Kinnan list) |
| 11.3 | Batch 2: Game Changers and EDHREC top 100 |
| 11.4 | Batch 3+: EDHREC top 500, mechanics as cards need them |

## Related
[[Curated cEDH Core Pool]] · [[cEDH Ground-Truth Data]] · [[ADR-0007 Card Definition Approach]]
