---
type: research
status: complete
tags: [card-definitions, research, decision]
updated: 2026-10-21
---

# Card Definition Spike Results

## Question
How should ~32,000 Commander-legal cards get behaviour? This note records the data from the [[Card Definition Spike Plan]] (steps A1–A6) for [[ADR-0007 Card Definition Approach]], scored with [[Evaluation Criteria]]. The decision is the user's.

## What was built (branch `m3-card-definitions`)
| Step | Artifact |
|---|---|
| A1 IR v1 draft | `crates/mtg-cards/src/ir.rs`. Abilities (keyword, spell, activated, triggered, static, replacement, escape); effect, filter, quantity, cost, condition, trigger, target and mana algebras; a closed set of named rule modifications; an explicit escape hatch. Serde-serializable, strict (`deny_unknown_fields` everywhere). |
| A2 DSL prototype | `cards/defs/*.yaml`, the IR's YAML form (single-key maps, not YAML tags). 40 cards: 20 staples and 20 of the hardest cEDH cards. Loader and validator in `crates/mtg-cards/src/defs.rs`. Files hold no Oracle text. |
| A3 Parser prototype | `crates/mtg-cards/src/parse.rs`, a hand-written compositional recursive-descent parser (~900 lines): sub-parsers for filters, targets, quantities, costs and mana composed into effect, trigger, keyword and static templates. Unparsed lines are reported, never approximated. |
| A4 Renderer lint | `crates/mtg-cards/src/render.rs`: IR → canonical English, compared with the Oracle line using canonical tokens. |
| A5 Comparison | `mtg dev card-spike --phase-coverage <coverage-data.json>` measures everything below in under a second on one core. |

## Findings

### Expressiveness (A2)
- **All 40 cards are expressible with zero Rust escapes.** The hard 20 are Thassa's Oracle, Necropotence, Opposition Agent, Mox Diamond, Food Chain, Underworld Breach, Rhystic Study, Mystic Remora, Kinnan, Grand Abolisher, Force of Will, Fierce Guardianship, Mana Vault, Chrome Mox, Lion's Eye Diamond, Smothering Tithe, Orcish Bowmasters, Blood Moon, Demonic Consultation and Tymna.
- **Honesty caveat:** six constructs exist mainly for one card. They're bespoke primitives, a soft form of escape:
  - `most_discarded_this_way` (Windfall)
  - `opponents_dealt_combat_damage_this_turn` (Tymna)
  - `type_produced_by_trigger` (Kinnan)
  - Opposition Agent's two rule mods plus its search replacement
- Authoring added four general constructs: `exile … remember`, `move`, `reveal_until`, and discard with a filter.
- "Expressible" isn't "correct" until the engine interprets the IR (Phase B).
- **DSL robustness:**
  - Typos inside enum variants were silently accepted until `deny_unknown_fields` was added to every IR enum. A strict schema is essential for AI-authored files.
  - `yaml_serde` writes enums as YAML tags (`!activated`) by default; single-key maps are far easier to write.

### Parser coverage (A3)
| Set | Our prototype: cards fully parsed | Lines parsed | phase.rs "supported" (self-reported) |
|---|---|---|---|
| Curated cEDH pool (205) | 37 (18%) | 89 / 309 (29%) | 202 / 205 (99%) |
| Seeded 1,000-card sample | 100 (10%) | 413 / 1,855 (22%) | 893 / 981 found (91%) |
| All Commander-legal (32,116) | 3,090 (9.6%) | 13,888 / 60,470 (23%) | 28,903 / 31,270 found (92%) |

- About 900 lines of grammar cover ~23% of all Oracle lines.
- **The most common unparsed lines are cheap:** "enchant creature" (892), "equip {N}" (536), "choose one —" (327), cycling (220), crew (180), "equipped creature gets +N/+N" (172), devoid, convoke, flashback, kicker, prowess, "can't block", "can't be countered".
- phase.rs shows ~92% is reachable with a mature grammar, though its "supported" means "no gaps detected", not verified ([[phase.rs Evaluation]]).

### Renderer lint (A4)
- **97.8% of parsed lines round-trip:** rendering the IR reproduces the Oracle line's canonical tokens.
- **Injected mis-parses caught: 157 of 164 (95.7%).**

  | Mutation | Caught |
  |---|---|
  | Card type swapped | 48 / 49 |
  | Player swapped (you ↔ opponent) | 15 / 15 |
  | Number changed | 93 / 99 |
  | A "may" dropped | 1 / 1 |

  The misses are IR fields the renderer doesn't print, such as a search count of 1. Blind spots like these must be eliminated for the lint to be trusted.
- **Hand-authored YAML: 19 of 40 render to their Oracle text.** Most mismatches are renderer gaps (`look_at_top`, `move_cards`, `cant_cast`, alternative costs), not authoring errors. Two show the lint's value:
  - Force of Will's missing alternative-cost line is flagged.
  - Demonic Tutor's real wording ("put **that card** into your hand") differs from the template the prototype assumed ("put it").

## Scoring (Evaluation Criteria; scores are judgment informed by the data above)
| Criterion (weight) | A. DSL + escape | B. Parser only | C. Code per card | **D. Hybrid** | E. Convert phase.rs output |
|---|---|---|---|---|---|
| Correctness / verifiability (25%) | 4 | 3 | 3 | **4** | 2 |
| Velocity to full pool (20%) | 2 | 4 | 1 | **4** | 5 |
| Maintenance under errata and CR changes (15%) | 2 | 5 | 1 | **4** | 2 |
| Runtime performance (10%) | 4 | 4 | 4 | **4** | 4 |
| AI-authorability (10%) | 5 | 3 | 3 | **5** | 2 |
| Expressiveness (10%) | 4 | 3 | 5 | **5** | 4 |
| Upfront cost (10%) | 4 | 2 | 4 | **3** | 3 |
| **Weighted** | 3.40 | 3.50 | 2.70 | **4.10** | 3.10 |

Why each option scored as it did:
- **E** (translating phase.rs's parsed abilities into our IR) wins on velocity. It loses on verifiability: it means importing a foreign IR with known silent drops (its PR #6526), plus upstream churn.
- **A alone** doesn't scale to 32k files or errata.
- **B alone** has no answer for the long tail.

## Recommendation (for the user to decide)
**Option D, hybrid**, with these specifics from the spike:
1. **Parser first.** Grow our compositional grammar by unparsed-shape frequency (the list above), porting phrase coverage from phase.rs's grammar where useful under ADR-0012 E3, with attribution. Parsed cards start as `parsed`.
2. **YAML overrides** (the IR's YAML form, strict schema) for the long tail and the cEDH pool's hardest cards. AI-authored and reviewed.
3. **Rust escapes** only for the truly unique (Shahrazad, Karn Liberated, Mindslaver). None were needed in the 40-card set.
4. **Mandatory renderer lint** on every definition from every front-end, extended until it has no blind spots. A lint mismatch blocks `verified`.
5. **`verified` requires scenario tests** per ability (unchanged from [[Card Data Model]]). phase.rs's coverage is a differential oracle (Q24) that flags disagreements for review.
6. **Optional, time-boxed experiment at the start of Phase B:** an importer from phase.rs's `card-data.json` that emits our IR as **`parsed`, never `verified`**. If its output passes the renderer lint at high rates, it fast-tracks breadth; if not, drop it.

## Open follow-ups
- LLM authoring was measured only with this session's agent, which also designed the IR, so the result is biased. An independent agent eval (Oracle text plus IR docs → YAML, scored by scenario tests) belongs in M8's agent evals.
- Velocity per card can't be measured until the engine interprets the IR (Phase B).

## Sources
- Spike code and data: `crates/mtg-cards/src/{ir,defs,parse,render}.rs`, `cards/defs/*.yaml`, `crates/mtg-cli/src/spike.rs`, and the curated pool list `tests/fixtures/spike/curated-pool.txt`. Run with `mtg dev card-spike` against card DB `scryfall-2026-10-20#b1903564`.
- phase.rs published coverage, generated 2026-10-19 (MTGJSON 5.3.0+20261019): https://data.phase-rs.dev/coverage-data.json
- [[Oracle Text Corpus Analysis]], [[Evaluation Criteria]], [[phase.rs Evaluation]].
