---
type: architecture
status: proposed
tags: [architecture]
updated: 2026-10-20
---

# Architecture Overview

## The shape in one picture
```
                    ┌──────────────────────── Interfaces ────────────────────────┐
   AI agent ──MCP──▶│ mtg-mcp   │ mtg-cli (JSON/REPL) │ mtg-py │ mtg-wasm │ protocol│
                    └─────┬──────────────┬─────────────────┬──────────────────────┘
                          ▼              ▼                 ▼
                    ┌───────────── mtg-sim (experiments, runner, metrics, reports) ─────────────┐
                    │  spec → seat assignment → per-game seeds → parallel games → aggregate     │
                    └─────┬───────────────────────────────────────────────┬────────────────────┘
                          │ drives                                        │ observes
                          ▼                                               ▼
   ┌──── mtg-policy ────────────────┐   PendingDecision    ┌────────── mtg-engine ──────────┐
   │ Policy trait, composition       │◀────────────────────│ GameState + rules procedures    │
   │ baselines │ playbook │ search │ │────────────────────▶│ (turns, priority, stack, SBAs,  │
   │ llm-bridge                     │      Choice          │  triggers, replacements, layers)│
   └────────────────────────────────┘                      │ emits Events                    │
                                                           └───────────┬────────────────────┘
                                                                       │ reads
                                                           ┌───────────▼────────────────────┐
                                                           │ mtg-cards: compiled card IR     │
                                                           │ (from DSL / parser / escapes)   │
                                                           └───────────┬────────────────────┘
                                                                       │ built from
                                                           ┌───────────▼────────────────────┐
                                                           │ mtg-data: Scryfall/MTGJSON      │
                                                           │ ingestion → versioned card DB   │
                                                           └────────────────────────────────┘
```

## Core loop
The engine is a **pure-ish state machine** ([[ADR-0004 Engine as Decision State Machine]]):

```rust
// Illustrative only; the real API is designed in M1/M2.
loop {
    match game.advance() {                        // runs rules procedures until input is needed
        Step::NeedsDecision(pending) => {
            let view   = game.view_for(pending.player);          // redacted
            let choice = policies[pending.player].decide(&view, &pending);
            game.apply(choice)?;                  // validated against pending.options
        }
        Step::GameOver(result) => break result,
    }
}
```
- `advance()` performs turn-based actions, SBAs (CR 704.3), puts triggers on the stack, and so on, until a player must choose something.
- Every rule procedure that needs player input **suspends** by returning a `PendingDecision`. There are no callbacks into policies from deep inside rules code, which keeps the engine re-entrant, cloneable, serializable, and easy to search over. See [[Decision & Action Model]].
- Every state change emits **events** ([[Event System]]) for triggers, metrics, logs, and judge explanations.

## Key architectural decisions
| Concern | Approach | Note |
|---|---|---|
| Control flow | Explicit decision suspension (continuation stored in state) | [[Decision & Action Model]] |
| State representation | Arena of objects with generational IDs. Persistent vs. mutable-clone is TBD by benchmark | [[Game State Model]], [[Performance & Cheap Cloning]] |
| Card behaviour | Compiled typed IR interpreted by the engine. The authoring front-end is TBD | [[ADR-0007 Card Definition Approach]] |
| Continuous effects | Recompute characteristics through layers (CR 613) with caching and invalidation | [[Continuous Effects & Layers]] |
| Randomness | Single seeded RNG stream per game, owned by the state | [[Determinism & Replay]] |
| Hidden information | Engine holds full truth. `view_for(player)` redacts | [[Hidden Information & Player Views]] |
| Parallelism | One game per worker. Card DB shared via `Arc` | [[Batch Runner & Parallelism]] |

## Related
[[Crate Layout]] · [[Subsystems Index]] · [[Design Principles]]
