---
type: milestone
status: in-progress
milestone: M3
tags: [milestone]
updated: 2026-10-21
---

# M3: Card Definitions v1

**Goal:** decide how cards are defined, and get the first 100 non-trivial cards verified.

## Checklist
- [x] Run the [[Card Definition Spike Plan]] → [[Card Definition Spike Results]]. **Accepting [[ADR-0007 Card Definition Approach]] is the user's call** (D recommended)
- [ ] IR v1: effects, filters, costs, triggers, statics, replacements (the shape at least; behaviour arrives in M4/M5)
- [ ] Chosen front-end(s) implemented (parser, DSL loader, escape registry)
- [ ] IR → text renderer (lint against Oracle)
- [ ] Targeting & legality ([[Targeting & Legality]])
- [ ] Evergreen keywords ([[Keywords & Keyword Actions]]): flying, reach, trample, first and double strike, deathtouch, lifelink, vigilance, haste, menace, defender, flash, hexproof, shroud, indestructible, ward, protection
- [ ] Counters ([[Counters]])
- [ ] Greedy baseline policy ([[A - Baseline Policies]])
- [ ] 100 cards verified (removal, counterspells, card draw, tutors, mana rocks)

## Exit criteria
ADR-0007 accepted. 100 cards `verified`. A random-policy fuzz over those cards is clean.

## Plan
M3 has two phases with a decision between them. The card-definition approach is **yours to decide** (ADR-0007 is research-first by your request), so Phase A produces the data and stops.

### Phase A: card-definition spike (→ ADR-0007 recommendation)
| # | Work | Output |
|---|---|---|
| A1 | **IR v1 draft** in `mtg-cards::ir`: abilities (spell, activated, triggered, static, replacement, mana, keyword) plus effect, filter, quantity, cost, condition, duration and target algebras. Serde-serializable, so the IR's YAML form *is* the DSL candidate. | Rust types plus docs |
| A2 | **Expressiveness:** hand-author about 40 curated-pool cards as IR YAML (20 easy, 20 of the hardest: Thassa's Oracle, Necropotence, Opposition Agent, Mox Diamond, Food Chain, Underworld Breach…). Count escape hatches. | `cards/defs/*.yaml`, expressiveness score |
| A3 | **Parser prototype** (`winnow`) for the most common phrase families (keywords, mana abilities, "deal N damage to target …", draw, destroy/exile target, ETB and "whenever" triggers, activated-ability costs). Measure the parse rate per line and per card on the curated pool and on a seeded 1,000-card Commander-legal sample. | Coverage numbers |
| A4 | **IR → English renderer:** diff rendered text against Oracle for every parsed card, and measure how many **injected mis-parses** the lint catches (silent mis-parses are the main risk). | Lint catch rate |
| A5 | **phase.rs comparison** on the same sample, using its published per-card coverage (ADR-0012 E3; differential oracle Q24). | Comparison table |
| A6 | **Score** the options with [[Evaluation Criteria]] and write the recommendation into ADR-0007. **Stop for your decision.** | ADR-0007 ready to decide |

### Phase B: the engine executes the IR (plan, 2026-10-21)
ADR-0007 is still open, but its fixed constraint is that **every option compiles to the same typed IR** (`mtg_cards::ir`). So Phase B teaches the engine to execute that IR, and none of it depends on which front-end you choose. Front-end work (a production parser, a DSL loader, an escape registry) waits for your decision.

#### Design decisions
| # | Decision | Why |
|---|---|---|
| B-D1 | **The engine consumes only `mtg_cards::ir`.** Verified cards are defined in `cards/defs/*.yaml`, the IR's serialized form, which every ADR-0007 option produces in some way. Characteristics (cost, type line, colors, P/T) come from the local card DB and are never committed. | Front-end neutral. Legally clean. |
| B-D2 | **Support check at load time.** Building a `CardDef` walks the IR and rejects any node the engine can't execute yet, naming what's missing ("triggered abilities arrive in M4"). There are no partially working cards. | Fail loudly (M2 lesson 4). Feeds the coverage report. |
| B-D3 | **Resumable effect interpreter.** `Task::Effect` holds a context: controller, source, targets and whether each is still legal, X, chosen modes, the bound "each" player, remembered objects, variables and last-known information. It also holds a stack of *paths* into the immutable IR (definition, ability, child indices). | Clones stay cheap and the IR stays in the shared pool (ADR-0004). |
| B-D4 | **Every choice stays one-of-N** (M2 D1). Searches, scry, look-at-top, discards, sacrifices, modes, targets and colors are all factored this way. Targets are chosen one at a time per slot, with a "done" option for "up to" slots. Ordering cards means picking the next card to place. | Same contract for every client. |
| B-D5 | **"Choose a card name"** offers the names in the game's card pool plus one option for "a name not in this game". | Outcome-complete without listing about 30,000 names. |
| B-D6 | **Casting follows CR 601.2b–h:** modes, then alternative cost, then X, then hybrid/Phyrexian announcements (601.2b), then targets (601.2c), then mana payment, then non-mana costs (601.2h). A choice is offered only while a complete cast still exists. | Exact castability (extends M2 D7). Legality by construction. |
| B-D7 | **A stack item is a spell or an ability.** Activated abilities (CR 602) use the casting procedure. Once on the stack an ability is independent of its source (CR 113.7a), and it uses last-known information when the source is gone (608.2h). | One procedure; correct LKI. |
| B-D8 | **Generalized mana.** A mana ability's output is a set of alternatives, each a list of unit masks: Sol Ring is one alternative of {C}{C}, Birds of Paradise one unit of any color, "any one color ×3" five alternatives. Exact payability uses a DFS over alternatives plus Kuhn matching. Automatic plans use abilities whose only cost is {T}. Other mana abilities (sacrifice, life, discard, mana costs) are activated manually. A mana ability's own mana cost is paid from the pool (float first), which has the same outcome as nesting under CR 605.3a. Plans fix colors; manual and priority activations ask. | Covers rocks, dorks and rituals. Keeps M2's plan UX. |
| B-D9 | **P/T changes and counters:** until-end-of-turn P/T effects are a small list cleared in cleanup (CR 514.2), and counters (CR 122) are stored per object. M3 has only layer-7c additions (CR 613.4c), which commute. The full layer system comes in M5. | Enough for pump and -X/-X removal. |
| B-D10 | **Tokens:** predefined tokens (CR 111.10) and simple creature tokens get pool definitions at load time. Tokens off the battlefield cease to exist (CR 704.5d). | Treasure, Beast Within, Swan Song. |
| B-D11 | **Keywords in M3:** flash (702.8a), hexproof (702.11b), shroud (702.18a), indestructible (702.12b), on top of M2's set. **Ward and protection move to M4.** Ward is a triggered ability (702.21a), and protection needs damage prevention (702.16). | They depend on M4 machinery. |
| B-D12 | **Greedy policy:** sees only the `PlayerView`. Plays a land, casts the most expensive castable spell, aims harmful effects at opponents (the opponent with the highest life first) and helpful ones at itself, attacks when the attacker survives or the defender has no untapped blockers, and takes the first payment plan. | A non-random baseline for the M7 statistics. |
| B-D13 | **Verified** = a YAML definition the engine supports, plus CR-cited scenarios for each ability, all passing against the real DB. Coverage lists the scenario files. CI runs the synthetic-card tests; real-card scenarios run when the local DB exists (R13). | NFR-C1 without committing card data. |

#### Phases
| # | Work | CR | Done when |
|---|---|---|---|
| B1 | `mtg-engine` depends on `mtg-cards`. `CardDef` = characteristics plus `Vec<ir::Ability>`; keyword bitset and mana-ability index derived. Intrinsic land abilities as IR (305.6). Support check (B-D2). Synthetic test set re-expressed in IR. | 305.6, 605.1a | All M2 tests pass and golden replays are **unchanged** (behaviour preserved) |
| B2 | Effect interpreter (B-D3) for every IR effect that needs no triggers or replacements: damage, destroy, exile, move, counter, draw, discard, life, mana, search, shuffle, scry, look-at-top, move-cards, tap/untap, tokens, sacrifice, pump, may, if, if-does, unless, pay, for-each-player, choose, reveal-until, win. Quantities and conditions with LKI. | 608.2, 608.2b–c, 608.2h, 609.3, 701.6, 701.8, 701.13, 701.21–24 | Synthetic-card scenario per effect |
| B3 | Casting rework (B-D6): modes, alternative and additional costs, X, hybrid and Phyrexian, multi-slot and "up to" targets, spell targets, target legality with hexproof and shroud, resolution with partial target legality. | 601.2a–i, 115, 118.9, 107.3, 107.4e–f, 700.2 | Castability property test: every offered option completes |
| B4 | Activated abilities on the stack, with costs ({T} with summoning sickness, sacrifice, life, discard, exile) and timing and once-per-turn restrictions. Generalized mana abilities (B-D8). | 602, 605, 302.6, 602.5b, 113.7a | Mana-rock and fetch-land scenarios |
| B5 | Keywords flash, hexproof, shroud, indestructible; "can't be countered". | 702.8, 702.11, 702.18, 702.12 | Keyword scenarios |
| B6 | Counters, P/T effects, tokens, and SBAs 704.5d and 704.5q. | 122, 613.4c, 111.10, 704.5d, 704.5q | Scenarios |
| B7 | Greedy policy in `mtg-policy`. | — | It beats random in a 1,000-game batch |
| B8 | Verification harness (real-DB scenarios, coverage `verified`) and **100 verified cards**: removal, counterspells, card draw, tutors, mana rocks and dorks, fetch lands. | per card | `mtg data coverage --verified` lists 100 |
| B9 | Low-power fuzz over decks of verified cards (2 threads, tracked tasks) and over synthetic IR cards in CI. Re-bless goldens with reasons. | — | Fuzz clean |

#### Out of scope for M3 (cards needing these stay `unsupported` with a named reason)
- Triggered abilities, delayed triggers and ward (M4).
- Replacement effects and prevention, including protection (M4).
- Static abilities other than keywords and "can't be countered" (M5, layers).
- The commander zone and commander color identity (M6).
- Activated abilities from zones other than the battlefield, such as cycling and Spirit Guides (later in M3 if time allows).
- Divided damage (601.2d).
