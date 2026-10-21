---
type: adr
status: proposed
tags: [adr, determinism]
updated: 2026-10-20
---

# ADR-0005: Determinism and Replay

## Decision (proposed)
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
