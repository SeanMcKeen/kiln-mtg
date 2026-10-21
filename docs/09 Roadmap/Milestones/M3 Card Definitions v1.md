---
type: milestone
status: planned
milestone: M3
tags: [milestone]
updated: 2026-10-20
---

# M3: Card Definitions v1

**Goal:** decide how cards are defined, and get the first 100 non-trivial cards verified.

## Checklist
- [ ] Run the [[Card Definition Spike Plan]] → accept [[ADR-0007 Card Definition Approach]]
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

## Plan (draft, written at the end of M2)
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

### Phase B: implementation (after ADR-0007 is accepted)
- Replace the temporary M2 card IR with IR v1 and a general effect interpreter (a resumable task, like the M2 procedures).
- Build the chosen front-end(s): parser, DSL loader, escape registry, plus the renderer lint.
- Targeting and legality (CR 115, 601.2c, 608.2b) beyond M2's two target kinds.
- Evergreen keywords: flash, hexproof, shroud, indestructible, ward, protection, on top of M2's set.
- Counters (CR 122), including +1/+1 and -1/-1 annihilation (CR 704.5q).
- A greedy baseline policy.
- **100 cards verified**, each with CR-cited scenarios: removal, counterspells, card draw, tutors, mana rocks.
- Fuzz over those cards, clean.
