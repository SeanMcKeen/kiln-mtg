---
type: adr
status: accepted
decided: 2026-10-20
tags: [adr]
updated: 2026-10-20
---

# ADR-0001: Rust as Engine Language

## Context
The engine must be fast enough for thousands of simulated games with search AI, cheap to clone, safe to parallelize, embeddable (Python, WASM, other clients), and able to model the CR's many tagged-union concepts exhaustively.

## Options considered
Rust, TypeScript, C#/.NET, Kotlin/Java. The user was shown the trade-offs (speed, cloning, WASM/FFI vs. iteration speed and ecosystem).

## Decision
**Rust.**

## Consequences
- **Good:**
  - performance and predictable memory
  - `enum` and `match` exhaustiveness for decisions, events and IR
  - `Send`/`Sync` parallelism
  - PyO3 and wasm-bindgen bindings
  - an official MCP SDK (`rmcp`)
  - a prior Rust MTG engine exists (phase.rs, [[ADR-0012 Relationship to phase.rs]])
- **Bad:** a steeper learning curve, longer compile times (mitigate with crate boundaries), and the toolchain isn't installed yet (M1).
