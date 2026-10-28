---
type: milestone
status: review
milestone: M11
tags: [milestone, cards]
updated: 2026-10-28
---

# M11: Full Card Pool

**Continuous from M6 onward.** This is the long tail.

## Checklist
- [x] Coverage dashboard (per format and mechanic; per set waits for set codes in the card records)
- [x] Prioritize by cEDH relevance (decklist corpus; EDHTop16 frequency waits for data access, Q43), then Game Changers, then EDHREC rank
- [x] Agent-assisted override authoring pipeline (ADR-0007 hybrid: the C4 loop below)
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

## Progress
| Phase | Status | Notes |
|---|---|---|
| 11.1 | ✅ 2026-10-28 | `kiln data coverage --dashboard` (`kiln-sim::coverage`), published [[Coverage Report]]. |
| 11.2 | ✅ 2026-10-28 | All 18 unsupported cards of the original Kinnan list verified; `kinnan.txt` is now fully verified. New engine features: modal activated abilities, search caps and graveyard search, Springleaf Drum's tap-a-creature cost (CR 701.26a), haste for {T} abilities (a real bug: hasty creatures couldn't use them), convoke (CR 702.51), costs' sacrificed objects with last known information (CR 400.7j; Eldritch Evolution), a player splitting searched cards (Intuition), suspend (CR 702.62; Delay). |
| 11.3 | ✅ 2026-10-28 | Game Changers 60.4% → 81.1%, EDHREC top 100 55.8% → 92.6%. Features: conditional enters-tapped (check, battle, slow and bond lands), search to two destinations, no maximum hand size, flashback (CR 702.34a), "can't be blocked" (CR 509.1b), spell and static cost reductions (CR 601.2f), damage to each creature, search remembering what it found (CR 400.7j), untapping during others' untap steps (Seedborn Muse), "life lost this way", the opponent count, "up to" and "different names" searches. |
| 11.4 | ongoing | The long tail: EDHREC top 500 is 49.5% verified. Remaining Game Changers need bigger features: The One Ring, Jeska's Will, Narset, Field of the Dead, Glacial Chasm, The Tabernacle, Biorhythm, Coalition Victory, Panoptic Mirror, Tergrid (MDFC). |

## Results (2026-10-28)
- **Exit measure (C3 proxy):** 99.9% of the decklist corpus's appearances are verified (785 of 786; the one unsupported card is The One Ring, put in `kinnan-unsupported.txt` on purpose for the coverage-gate test). Game Changers 81.1%, EDHREC top 100 92.6%. The criterion as written needs tournament lists (Q43).
- **Verified cards:** 278 → 402 (124 more) with 0 unsupported definitions and 0 failing scenarios; overall Commander coverage is published in [[Coverage Report]] (1.3% verified, 9.8% runnable of 32,116 legal cards).
- **Bugs found by fuzzing the new cards** (each with a regression scenario): a spell could be offered when only discarding itself paid its additional cost (CR 601.2a); an Aura entering with the permanents it could enchant attached to one (CR 303.4g); loop shortcuts spent restricted mana (CR 106.6); Tainted Pact moved a commander to hand without its owner's choice (CR 903.9b); a mandatory life loop against a player who can't lose never ended (now a draw, CR 104.4b); "spells your opponents cast" cost increases judged an exiled card by its owner, not its caster (CR 601.2f). Fuzz runs: 0 violations over 1,500–2,000-game commander and two-color batches on several seeds.

## Related
[[Curated cEDH Core Pool]] · [[cEDH Ground-Truth Data]] · [[ADR-0007 Card Definition Approach]]
