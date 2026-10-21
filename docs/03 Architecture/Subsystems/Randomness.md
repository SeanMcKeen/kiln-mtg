---
type: subsystem
status: draft
cr: ["701.24", "705", "706", "103.1"]
impl_status: not-started
milestone: M2
tags: [subsystem, randomness]
updated: 2026-10-20
---

# Randomness

## Purpose
All game randomness: shuffling (CR 701.24), coin flips (CR 705), die rolls (CR 706), random selection (random discard, e.g. Gamble), and determining the starting player (CR 103.1).

## Responsibilities
- A **single seeded RNG** per game, owned by `GameState` ([[Determinism & Replay]]).
- Shuffling uses Fisher–Yates over the RNG, which is unbiased.
- **Shuffle invalidates knowledge** ([[Hidden Information & Player Views]]).
- "Random order" for cards put on the bottom.
- Die rolls with modifiers (natural result vs. modified, CR 706.2).
- Starting player: random by default, or seeded or fixed through the experiment spec.

## Notes
- **Policy RNG is separate** from game RNG, so policy changes don't perturb shuffles.
- Search AI determinizations use their own RNG stream as well, never the game's.

## Related
[[Determinism & Replay]] · [[Hidden Information & Player Views]]
