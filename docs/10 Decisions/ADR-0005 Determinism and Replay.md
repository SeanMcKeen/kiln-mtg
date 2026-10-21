---
type: adr
status: accepted
tags: [adr, determinism]
updated: 2026-10-21
---

# ADR-0005: Determinism and Replay

## Decision
- One seeded ChaCha RNG per game, owned by the state. Policies get separate RNG streams.
- Per-game seeds are derived by hashing the experiment seed with the game index.
- No `HashMap` iteration, wall-clock time or floats in game logic.
- A replay is `(setup, versions, seed, [(decision_id, option_index)])`.
- Reports are bit-identical for the same inputs. Search AI defaults to iteration budgets.

See [[Determinism & Replay]] and [[Reproducibility]].

## Consequences
- **Enables:** golden-replay regression tests, paired A/B experiments, and bug reproduction from a seed.
- **Costs:** discipline in code review, plus a CI determinism check that replays a sample of fuzz games.

**Accept by:** M2.

## M2 evidence (2026-10-21)
- One ChaCha8 stream per game, with our own bounded sampling and shuffles. Per-game and per-seat seeds come from splitmix64. There's no `HashMap`, wall-clock time or floating point in the engine.
- Replay is `(setup, choices as (decision_id, option))` plus the final state hash.
- **Results:** 1,000 of 1,000 sampled fuzz games replayed bit-identically. Batch results are identical on 1 and 18 threads. Three golden replays are verified in CI.

**Status: accepted (2026-10-21)** by the user, on this evidence.
