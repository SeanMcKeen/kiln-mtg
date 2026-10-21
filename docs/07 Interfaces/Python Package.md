---
type: design
status: draft
tags: [interfaces, python]
updated: 2026-10-20
---

# Python Package (M12)

`pip install mtg-engine` (name TBD). PyO3 + maturin wheels for macOS, Linux and Windows.

## Use cases
1. **Run experiments and analyze results** in notebooks: `run = mtg.sim.run("spec.yaml")` → `run.games_df()` (pandas/polars) for per-game and per-card tables. Parquet artifacts load directly.
2. **Custom policies in Python:** implement `decide(view, decision)`. It's slower (GIL and FFI per decision), so it suits research rather than bulk runs. Batched decision APIs can reduce overhead.
3. **RL research:** a gym-style environment (`reset`/`step`) over `PendingDecision`, with action masking from the legal-option list. Observation encoding is an open design question.

## Notes
- Release the GIL during engine-only stretches, and run batches in Rust threads.
- Keep it thin: logic stays in Rust.

## Related
[[Library API]] · [[Policy Interface]]
