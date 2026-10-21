---
type: subsystem
status: draft
cr: ["106", "107", "118", "202", "605"]
impl_status: not-started
milestone: M2
tags: [subsystem, mana, costs]
updated: 2026-10-20
---

# Costs & Mana

## Purpose
This subsystem covers mana, mana pools, mana abilities, cost determination and payment.

## CR sections
106 (mana), 107 (numbers and symbols), 118 (costs), 202 (mana cost and color), 605 (mana abilities).

## Responsibilities
- **Mana pool** per player, with **restricted/conditional mana** ("spend only on artifact spells", "can't be spent on…") and riders ("when you spend this mana…"). Pools empty between steps (CR 500.5) unless an effect says otherwise.
- **Mana symbol algebra:** generic, colored, colorless {C}, hybrid, monocolored hybrid {2/W}, Phyrexian {W/P}, snow {S}, X, and the half mana of un-cards (not supported).
- **Cost modification:** increases (Thalia, Thalia's Lieutenant-style taxes, commander tax), reductions, "without paying its mana cost", alternative costs, additional costs. Applied in CR 601.2f order.
- **Mana abilities** (CR 605): they don't use the stack and resolve immediately (CR 605.3b). They can be activated during cost payment.
- **Non-mana costs:** tap/untap symbols (CR 107.5–107.6), sacrifice, discard, exile, pay life, remove counters, return to hand.

## Decisions raised
- `PayCost` (plan-based) or step-wise `ActivateManaAbility`. See [[Decision & Action Model]] R2.
- Choice of Phyrexian payment (life vs. mana) and hybrid halves.

## Payment-plan enumeration (design problem)
Given a cost and available sources, enumerate **outcome-distinct** payment plans. Two plans are equivalent if they leave the same untapped permanents, pool contents, life and graveyard, among other things.
- Lands and rocks with multiple abilities (City of Brass deals damage, Ancient Tomb deals damage, Mana Confluence costs life).
- Conditional sources (Gemstone Caverns, Mox Diamond, Chrome Mox imprint color).
- **cEDH reality:** players float mana, use rituals (Dark Ritual), and treat mana rocks with abilities (Grim Monolith, Mana Vault's upkeep damage) as resources. Plan enumeration must not hide strategically different plans.

Leaning: a greedy canonicalizer enumerates plans up to a cap and always includes a "manual" escape option. To be benchmarked. → [[12 Open Questions]]

## Large numbers
Infinite-mana combos (Basalt Monolith plus Rings of Brighthearth / Power Artifact, Kinnan) produce arbitrarily large mana. Pools and counts need a representation that doesn't overflow: `u64` with saturating arithmetic plus a "shortcut multiplier", or bignums. See FR-E11 and [[Loops & Shortcuts]].

## Test scenarios
- Mana Vault ("doesn't untap", upkeep damage trigger).
- Paying {2/W}{W/P} with various resources.
- Gemstone Caverns: a luck counter conditionally makes it produce any color.

## Related
[[Casting & Activating]] · [[Loops & Shortcuts]]
