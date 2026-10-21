---
type: architecture
status: draft
tags: [architecture, performance, search]
updated: 2026-10-20
---

# Performance & Cheap Cloning

## Why cloning matters
[[D - Search AI]] (ISMCTS) clones the game state thousands of times per decision: once per determinization and simulation. With a 10 ms budget per decision, a 10 µs clone leaves room for about 1,000 playouts minus playout cost. So clone cost directly sets play quality per second.

## Candidate strategies
| Strategy | Clone cost | Mutation cost | Notes |
|---|---|---|---|
| **A. Plain structs + `Vec` arenas, `#[derive(Clone)]`** | O(state size). Likely ~2–20 µs for cEDH boards | Fastest | Simple. Keep the state compact: `SmallVec`, interned IDs, the card IR behind `Arc` (shared, never cloned). |
| **B. Persistent data structures** (`rpds`, `im`) | O(1) with structural sharing | Slower (pointer chasing, refcounts) | Used by phase.rs ("rpds persistent state"). Great for undo and branching. |
| **C. Hybrid** | Small mutable core plus `Arc`-shared big immutable parts (card DB, effect definitions, history) | Fast | Likely the sweet spot. |
| **D. Undo log** (make/unmake moves) | ~0 | Requires reversible operations | Classic in chess engines. Very hard with MTG's complexity, so not recommended. |

**Leaning: C.** Measure A vs. B on real cEDH states in M2 before committing. → [[12 Open Questions]]

## Other performance levers
- **Characteristics caching:** layer computation (CR 613) is expensive. Cache derived characteristics and invalidate on relevant changes (new effect, zone change, counter change, timestamp change).
- **Trigger indexing:** index trigger conditions by event type so each event checks only relevant triggers.
- **Trivial-decision fast path:** see [[Decision & Action Model]] R6.
- **Interning:** card names, types, and subtypes as integer IDs. The card IR is immutable and shared.
- **No allocations in the hot loop:** reuse scratch buffers per worker.
- **Parallelism across games, not within games:** embarrassingly parallel ([[Batch Runner & Parallelism]]).

## Benchmarks to build (M2+)
1. A clone of a representative turn-5 cEDH board (4 players, ~60 permanents, 6 cards per hand, stack depth 3).
2. Random-policy full games per second per core.
3. Legal-action enumeration at priority.
4. Layer recomputation with ~20 continuous effects (Blood Moon, Humility-style stress).

Benchmarks use `criterion`, and results are tracked in the vault (`02 Requirements/Non-Functional Requirements` targets).

## Related
[[Game State Model]] · [[Non-Functional Requirements]] · [[Rust Ecosystem]]
