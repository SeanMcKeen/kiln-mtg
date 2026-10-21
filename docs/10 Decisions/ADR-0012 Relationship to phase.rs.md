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
