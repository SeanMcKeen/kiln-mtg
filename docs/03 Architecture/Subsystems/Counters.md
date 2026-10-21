---
type: subsystem
status: draft
cr: ["122"]
impl_status: not-started
milestone: M3
tags: [subsystem, counters]
updated: 2026-10-20
---

# Counters

## Purpose
This subsystem handles counters on objects and players: +1/+1, -1/-1, loyalty, poison, keyword counters, lore, time, charge, luck, rad, and more.

## CR sections
122 (counters), 704.5q (+1/+1 and -1/-1 annihilation, also 122.3), 704.5r (counter limits), plus poison (104.3d) and rad counters (728).

## Responsibilities
- Counters are **not objects** (CR 122.1). They're stored as `counter_kind → count` maps on objects and players.
- P/T counters apply in layer 7c (CR 613.4c). Keyword counters (flying, deathtouch, …) apply in layer 6 (CR 613.1f).
- "Enters with N counters" is an ETB replacement (CR 614.1c), and also counts as counters being "put on" it (CR 122.6).
- Proliferate (701.34) chooses any subset of permanents and players with counters, so it raises a decision.

## Decisions raised
Proliferate subset, counter removal choices ("remove a counter from among permanents you control"), and Walking Ballista's X.

## Edge cases
- Walking Ballista + Heliod (lifelink, gain life, put a +1/+1 counter, remove it, ping): an optional loop, so shortcut.
- Gemstone Caverns' luck counter, Mox Amber, Sagas (lore counters, SBA 704.5s).

## Related
[[Continuous Effects & Layers]] · [[Loops & Shortcuts]]
