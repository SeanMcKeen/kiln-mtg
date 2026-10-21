---
type: adr
status: proposed
tags: [adr, architecture]
updated: 2026-10-20
---

# ADR-0004: Engine as a Decision/Choice State Machine

## Context
- The user's fidelity requirement: illegal actions are impossible, and every legal choice is customizable.
- Search AI needs cloneable mid-decision states.
- MCP, Python and WASM need serializable, resumable games.

## Options
1. **Callbacks:** the engine calls `player.choose(...)` deep inside rules code. This is the classic Forge/XMage style. It's simple to write, but impossible to clone or serialize mid-call, and awkward for async agents.
2. **Coroutines/async:** rules code `await`s decisions. Readable, but Rust futures can't be cloned or serialized.
3. **Explicit state machine:** the engine returns `PendingDecision` and stores the continuation in `GameState`. `apply(choice)` resumes it.

## Decision (proposed)
**Option 3.** All rules procedures that need input are written as resumable state machines. Clients only see `PendingDecision → Choice`. See [[Decision & Action Model]].

## Consequences
- **Good:** clone, serialize and replay at any point. Uniform for every client. Testable.
- **Bad:** more verbose rules code, with procedures written as explicit steps. Mitigate with helper macros, or a small internal procedure VM (open question).

**Accept by:** M2, after prototyping casting and combat in this style.

## M2 evidence (2026-10-21)
Casting (CR 601.2a–i), payment, combat (factored attacks and blocks, per-point damage division), mulligans and SBAs are all written as resumable tasks on a stack stored in the game state (R10).
- **Clone/serialize at any point:** a clone of a mid-game state costs 377 ns, and clone-then-apply equals apply on every decision tested.
- **Legality by construction:** a property test applies *every* offered option. Over 1M fuzz games (~4 billion decisions), the engine never rejected an offered option.
- **Verbosity:** acceptable. Each procedure is a small task enum plus `step`/`answer` methods.

**Status: ready to accept.** This is the user's call ([[12 Open Questions]] Q2).
