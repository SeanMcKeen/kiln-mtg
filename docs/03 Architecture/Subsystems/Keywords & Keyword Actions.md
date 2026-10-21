---
type: subsystem
status: draft
cr: ["701", "702"]
impl_status: not-started
milestone: M3
tags: [subsystem, keywords]
updated: 2026-10-20
---

# Keywords & Keyword Actions

## Purpose
This subsystem implements keyword abilities (CR 702) and keyword actions (CR 701) as reusable building blocks that card definitions reference.

## Scale (2026-06-19 CR)
- **69 keyword actions** in 701 (counter, destroy, exile, sacrifice, scry, shuffle, mill, surveil, proliferate, transform, …).
- **194 keyword abilities** in 702 (deathtouch, flying, ward, storm, escape, partner, …).
- Scryfall lists **814 distinct "keyword" strings** across Commander-legal cards. These include ability words such as "Landfall", which have no rules meaning, so the real count is lower ([[Oracle Text Corpus Analysis]]).

## Prioritization
Implement in order of frequency and cEDH relevance. The most common across Commander-legal cards (Scryfall `keywords`):
1. Flying (3,164), Enchant (1,241), Trample (994), Vigilance (727), Haste (640), Flash (608), Equip (601), Mill (581), Scry (425), Reach, Transform, Cycling, Menace, First strike, Lifelink, Treasure, Deathtouch, …
2. cEDH-critical regardless of frequency: storm (702.40), escape (702.138), partner (702.124), flash, split second (702.61), ward (702.21), cumulative upkeep (702.24), convoke (702.51), delve (702.66), affinity (702.41), flashback (702.34), cascade (702.85), kicker (702.33), overload (702.96), phasing (702.26).

## Implementation pattern
Most keyword abilities expand into **standard triggered, static, activated or replacement abilities**: flashback is a casting permission plus an exile replacement, and persist is a dies trigger with a condition. The IR should represent keywords **as named macros over primitive IR**, which keeps the engine core small. Some keywords need engine hooks:
- evasion: flying, menace, landwalk
- combat-damage modifiers: deathtouch, trample, first strike, double strike, lifelink
- protection

## Coverage tracking
Each keyword gets an entry in `Rules Coverage.base` with `impl_status`.

## Related
[[Card Definitions Problem Statement]] · [[Combat]] · [[Targeting & Legality]]
