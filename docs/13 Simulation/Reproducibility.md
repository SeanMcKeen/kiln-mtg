---
type: design
status: proposed
tags: [simulation, reproducibility]
updated: 2026-10-20
---

# Reproducibility

## Guarantee
Same `(engine version, card DB version, resolved spec, seed)` gives a **bit-identical** `report.json` and identical replays, independent of thread count and machine (NFR-D1).

## How
- Game RNG and policy RNG are separate and derived from the experiment seed ([[Determinism & Replay]]).
- Search AI uses an **iteration budget** by default. A wall-clock budget is allowed but marks the run `reproducible: false`.
- LLM policies are inherently non-reproducible. Their recorded choices make the *replay* reproducible, but a re-run isn't. Such runs are marked `reproducible: false`.
- Aggregation uses a fixed order ([[Batch Runner & Parallelism]]).
- Every report embeds its versions and the spec hash.

## Re-running a single game
`mtg game replay runs/<id>/replays/123.json` reconstructs the exact game. For a game not kept as a replay, `mtg sim rerun <run-id> --game 123` re-simulates it from the seed, if the run is reproducible.

## Related
[[Determinism & Replay]] · [[Golden Replays]]
