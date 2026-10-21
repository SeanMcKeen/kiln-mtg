---
type: index
status: living
tags: [subsystem, index]
updated: 2026-10-20
---

# Subsystems Index

Each rules subsystem has one spec, built from the [[Subsystem Spec Template]]. Every spec covers:
- the CR sections it owns
- responsibilities and the data it owns
- the **player decisions it raises**
- edge cases, especially cEDH-relevant ones
- test scenarios
- open questions

Frontmatter carries `cr` (owned sections), `impl_status`, and `milestone`, which feed `Rules Coverage.base`.

| Subsystem | CR | Milestone |
|---|---|---|
| [[Zones & Objects]] | 108–111, 400–408 | M1–M2 |
| [[Turn Structure]] | 500–514, 703, 724 | M2 |
| [[Priority & the Stack]] | 117, 405, 608 (with [[Resolution]]) | M2 |
| [[Casting & Activating]] | 601, 602, 116 | M2–M3 |
| [[Costs & Mana]] | 106, 118, 605 | M2 |
| [[Targeting & Legality]] | 115, 601.2c, 608.2b, 702.16 (protection), 702.18 (shroud), 702.11 (hexproof), 702.21 (ward) | M3 |
| [[Resolution]] | 608, 609, 610 | M2 |
| [[State-Based Actions]] | 704 | M2–M4 |
| [[Triggered Abilities]] | 603 | M4 |
| [[Replacement & Prevention]] | 614–616 | M4 |
| [[Continuous Effects & Layers]] | 604, 611–613 | M5 |
| [[Copy Effects]] | 707 | M5 |
| [[Combat]] | 506–511 | M2 (basic) → M4 |
| [[Keywords & Keyword Actions]] | 701, 702 | M3 → M11 |
| [[Counters]] | 122 | M3 |
| [[Control & Ownership]] | 108.3, 110.2, 613.1b, 723 | M5 |
| [[Multiplayer]] | 800–806 | M6 |
| [[Commander]] | 903 | M6 |
| [[Two-Headed Giant]] | 805, 810 | M12 |
| [[Loops & Shortcuts]] | 104.4b, 732 | M4 |
| [[Randomness]] | 701.24 (shuffle), 705, 706 | M2 |
| [[Game End, Restarts & Subgames]] | 104, 727, 729 | M2 → M11 |

Also see [[CR to Subsystem Map]] for the reverse mapping.
