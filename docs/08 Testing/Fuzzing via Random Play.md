---
type: design
status: draft
tags: [testing, fuzzing]
updated: 2026-10-20
---

# Fuzzing via Random Play

Random-policy games ([[A - Baseline Policies]]) over random or real decks are the cheapest large-scale correctness check:
- **Nightly:** about 1M games across the verified card pool, with invariants enabled ([[Invariants & Property Tests]]).
- Any panic, invariant violation or replay divergence gets **minimized** (shrink the deck list and the choice log) and turned into a scenario test.
- Sampled replays are also run through `cargo-fuzz` targets for the Oracle parser and the scenario/spec loaders (malformed-input robustness).
- Coverage-guided variation: bias random policies toward rarely-taken option kinds, which surfaces untested paths.
