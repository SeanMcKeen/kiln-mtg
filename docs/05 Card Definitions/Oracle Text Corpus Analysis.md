---
type: research
status: draft
tags: [card-definitions, research, data]
updated: 2026-10-20
---

# Oracle Text Corpus Analysis

This is a quick quantitative look at the problem, to inform [[ADR-0007 Card Definition Approach]]. The data is the Scryfall `oracle_cards` bulk file dated **2026-10-20** (one object per Oracle ID).

## Pool size
| Measure | Count |
|---|---|
| Oracle objects in the bulk file | 38,940 |
| Excluding tokens, emblems, art series, planes, schemes, vanguards | 35,077 |
| **Commander-legal** | **32,116** |
| Legacy-legal | 31,958 |
| Modern-legal | 22,718 |
| Pauper-legal | 10,881 |
| Standard-legal | 5,164 |
| Vanilla (no rules text), Commander-legal | 340 |

### Layouts (all objects)
normal 33,511 · art_series 2,352 · token 926 · transform 394 · front_card 291 · planar 207 · saga 194 · adventure 167 · split 137 · vanguard 107 · scheme 102 · modal_dfc 99 · emblem 89 · double_faced_token 80 · **prepare 72** · class 35 · mutate 34 · flip 26 · leveler 26 · meld 21 · prototype 21 · host 20 · case 15 · augment 14

> [!note] New layouts
> `prepare` (CR 722, Preparation Cards) and `front_card` are recent additions to Scryfall. Ingestion must handle unknown layouts gracefully, by flagging them, never crashing ([[Ingestion Pipeline]]).

## Ability-line reuse
Method:
1. Split Commander-legal Oracle text (per face) on newlines.
2. Replace the card's own name with `~`.
3. Strip reminder text in parentheses.
4. Count distinct lines.

| Measure | Value |
|---|---|
| Total ability lines | 60,470 |
| **Distinct lines** | **36,037** |
| Distinct lines that appear on exactly one card | 32,115 (**89%** of distinct lines) |
| Coverage by the top 10 distinct lines | 11.6% of all lines |
| … top 100 | 22.0% |
| … top 500 | 31.0% |
| … top 1,000 | 35.4% |
| … top 5,000 | 48.7% |
| … top 10,000 | 56.9% |
| … top 20,000 | 73.5% |

**Most common lines:** Flying (2,548), Enchant creature (892), Trample (678), Flash (608), "This land enters tapped." (444), "{T}: Add {C}." (423), Vigilance (417), Haste (360), "Choose one —" (327), Menace (324), Reach, Defender, "Draw a card.", Deathtouch, First strike, "Equip {2}", Lifelink, "Cycling {2}", …

## Implications
1. **Whole-line templates don't scale.** After the top ~100 lines (mostly keywords and mana abilities), the curve flattens hard. 89% of distinct lines are unique, so any approach must be **compositional**: phrase-level grammar (costs, triggers, conditions, filters, effects, durations) combined freely.
2. **But the vocabulary is highly regular.** Wizards templates Oracle text consistently. The same sub-phrases ("target creature an opponent controls", "until end of turn", "for each", "you may", "if you do") recur constantly. A compositional parser can therefore achieve high coverage. phase.rs reports parsing ~34k cards with a nom-based parser (with gaps), and Arena's parser handles "80% or so of newly written cards" ([[Prior Art - MTG Arena GRE]]).
3. **Silent mis-parses are the main risk.** phase.rs PR #6526 shows a token-creation clause silently lowered to "unimplemented" ([[Prior Art - phase.rs]]). Coverage must be **verified** (tests), not just "parsed".
4. **Keywords give cheap early wins.** Keyword-only lines are among the most frequent.

## Follow-up analyses (planned for the spike)
- Phrase-level n-gram analysis: which sub-phrases cover the most lines.
- Per-cEDH-pool coverage: how many of the [[Curated cEDH Core Pool]] lines are unique.
- Effect-verb frequency ("draw", "destroy", "exile", "create", "search", "counter", "untap", …) to prioritize IR primitives.

## Sources
- Scryfall bulk data API, `oracle_cards`, 2026-10-20: https://api.scryfall.com/bulk-data
- Analysis scripts were run ad hoc in this session. They should be committed as `tools/corpus-analysis` in M1.
