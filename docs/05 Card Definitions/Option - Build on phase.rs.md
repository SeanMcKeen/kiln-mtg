---
type: option
status: draft
tags: [card-definitions, option, prior-art]
updated: 2026-10-20
---

# Option E: Build on or Fork phase.rs

## What
[phase.rs](https://github.com/phase-rs/phase) is an MIT/Apache Rust MTG engine with:
- a `nom`-based Oracle parser that lowers to typed `AbilityDefinition`s
- a pure `apply(state, action)` reducer with `rpds` persistent state
- an AI crate, WASM bindings, and a multiplayer server
- ~34k cards parsed from MTGJSON, with "thousands still unimplemented"

The license compatibility is **perfect**: same dual license. See [[Prior Art - phase.rs]].

## Sub-options
| Sub-option | Description |
|---|---|
| E1. Depend on the `engine` crate | Use it as a library. Contribute upstream. |
| E2. Fork it | Take the codebase and diverge toward simulation and agent priorities. |
| E3. Borrow selectively | Study and port the parser grammar and test corpus into our own IR. |
| E4. Ignore it | Clean-room design. |

## Considerations
- **Goal mismatch:** phase.rs is a *game client* first (Tauri app, PWA, multiplayer server). We're simulation-first. Our priorities are throughput, decision factoring, strict hidden-information views, deterministic replays, and a CR-cited test corpus. Its architecture may or may not fit. **Needs an actual code review.**
- **Velocity:** it has 8,000+ commits and 1.5k open issues, so it's very active. Depending on it means tracking a fast-moving upstream. Forking means losing its velocity.
- **Fidelity:** our "every legal choice is exposed" requirement must be checked against its decision model.
- **Reuse value is highest for the parser**, the most expensive asset to build.

## Recommendation
Do **not** decide yet. Run a time-boxed **phase.rs evaluation** in M1 ([[M1 Foundations & Data]]):
- clone it and read the engine and parser
- benchmark game throughput
- check the decision model against [[Decision & Action Model]]
- check hidden-information handling
- evaluate parser output quality on the [[Curated cEDH Core Pool]]

Outcome: [[ADR-0012 Relationship to phase.rs]].
