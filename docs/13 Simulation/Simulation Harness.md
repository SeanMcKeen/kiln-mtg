---
type: design
status: draft
tags: [simulation, headline]
updated: 2026-10-20
---

# Simulation Harness

> [!abstract] The product
> The harness turns "evaluate X vs Y, Z, C in a cEDH pod" into thousands of seeded, rules-exact games and a statistically honest report.

## Pipeline
```
Experiment Spec ──validate──► decks imported + coverage-gated ──► seating plan ──► game seeds
      │                                                                              │
      ▼                                                                              ▼
 metric flags ──► EventSink config            worker pool (rayon): for each game index i
                                                 game = Game::new(setup_i, db, seed_i)
                                                 loop advance → policy[seat].decide → apply
                                                 collect per-game record + metric events
                                                              │
                                                              ▼
                              streaming aggregator (order-independent) ──► Report + artifacts
```

## Components
| Component | Note |
|---|---|
| Spec format and validation | [[Experiment Spec]] |
| Deck import and coverage gate | [[Deck Import]], [[Card Data Model]] |
| Seating strategies | [[Statistics for Multiplayer Simulation]] |
| Per-seat policies | [[Policy Composition]] |
| Parallel runner | [[Batch Runner & Parallelism]] |
| Metrics | [[Metrics Catalog]] |
| Output | [[Output Formats & Report Schema]] |
| Reproducibility | [[Reproducibility]] |
| Validity | [[Validity & Caveats]] |

## Modes
| Mode | Description | Story |
|---|---|---|
| `pod` | N-player games, the default | US-1 |
| `goldfish` | One deck, opponents are passive "null" players (no cards, can't lose, or configurable) | US-2 |
| `ab` | Paired variants with common random numbers | US-3 |
| `opening_hands` | Shuffle, draw, apply mulligan policy, stop. Very fast | US-4 |
| `replay` | Re-run stored games, possibly with a different policy from a decision point | debugging |

## Game termination
Win/loss/draw per the CR, or **caps**: `max_turns` (default 30 for cEDH), `max_decisions`, and `max_wall_ms` per game. A capped game is recorded as `capped`, never as a rules draw. A high cap rate is a red flag in the report (it usually means a policy is looping without progress).

## Related
[[User Stories]] · [[M7 Simulation Lab v1]]
