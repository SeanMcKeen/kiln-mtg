---
type: design
status: draft
tags: [testing, replay, regression]
updated: 2026-10-22
---

# Golden Replays

A golden replay is a stored `kiln-replay/1` file ([[Determinism & Replay]]) plus an expected final-state hash and result.

## Sources
- Interesting games from simulation runs (upsets, complex combo turns).
- Bug reproductions.
- Hand-scripted "showcase" games for each milestone.

## Behaviour
- CI replays all golden files. On **divergence**, it reports the first differing decision (ID, expected option count vs. actual) and the event diff.
- An intentional change (a rules fix, a new CR) requires `kiln testkit rebless <file> --reason "CR 2026-xx change to 510.1c"`, which updates the hash and logs the reason in the file.
- Golden replays pin `engine`, `card_db` and `cr` versions in metadata. Card DB refreshes that change a card used in a replay trigger re-bless review.
