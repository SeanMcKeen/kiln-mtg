---
type: adr
status: open
tags: [adr, prior-art]
updated: 2026-10-20
---

# ADR-0012: Relationship to phase.rs

## Context
[phase.rs](https://github.com/phase-rs/phase) is an active (8k+ commits), MIT/Apache-licensed Rust MTG engine:
- an Oracle parser (nom) covering ~34k cards with gaps
- a pure reducer with persistent state
- an AI crate
- WASM
- a multiplayer server

It's the closest prior art, and its license is compatible. See [[Prior Art - phase.rs]] and [[Option - Build on phase.rs]].

## Options
E1 depend on it · E2 fork it · E3 borrow selectively (parser grammar, test corpus, ideas) · E4 clean-room.

## Status
**Open.** Decide after the time-boxed evaluation in [[M1 Foundations & Data]], against:
- the decision-model fit ([[Decision & Action Model]])
- hidden-information handling
- throughput benchmarks
- parser quality on the [[Curated cEDH Core Pool]]
- upstream velocity and maintainability

## Initial leaning
**E3 (borrow selectively)**, unless the evaluation shows its engine already meets our fidelity and throughput bars. In that case, E1 or E2 could save a year of work. **This is the highest-leverage open decision in the project.**

## Recommendation after the M1 evaluation (2026-10-20)
**E3: borrow selectively.** Full evidence is in [[phase.rs Evaluation]].

| Criterion | Result |
|---|---|
| Decision-model fit | Same state-machine architecture, but legality is advisory: 39 offered actions rejected in 20 random games. Attacks are declared jointly, and mana payment is auto by default. ⚠️ |
| Hidden information | Client redaction exists, but the AI reads the full state by default (`determinization_samples = 0`). ⚠️ |
| Throughput | Clone 8.9 µs ✅. `legal_actions` ~4 ms vs a 20 µs target, and ~12 s per random 4-player game vs 5 ms. ❌ |
| Parser quality on the cEDH pool | 201 of 205 parse without gaps, and spot-checked trees look faithful. ⭐ |
| Velocity and maintainability | ~1.7M non-test lines in the engine crate, a nightly toolchain, ~7 commits a day, client-first goals, silent drops of unresolved deck names. ⚠️ |

The engine doesn't meet our fidelity or throughput bars, so E1 and E2 would mean rewriting its core anyway. Its parser is the strongest asset in the field, so the M3 card-definition spike should treat it as a primary contender, and its coverage data should serve as a differential oracle.

**Status: still open.** The user decides (Q21). If accepted, this ADR becomes "Accepted: E3", with the scope above.
