---
type: research
status: draft
tags: [research, prior-art, rust]
updated: 2026-10-20
---

# Prior Art: phase.rs

**Repo:** https://github.com/phase-rs/phase. Its GitHub description is "A rules engine and game client — Rust + WASM + React". The license is MIT OR Apache-2.0. On 2026-10-20 the GitHub API showed about 307 stars and a push that day. The README lists 8k+ commits and about 1.5k open issues.

## What it is
- A Rust MTG engine that compiles to native and WASM. It powers a Tauri desktop app, a browser PWA, Android, and a WebSocket multiplayer server.
- **Crates:**
  - `engine`: types, game logic, parser, card database
  - `phase-ai`: evaluation, legal actions, search, and "per-card decision logic"
  - `engine-wasm`: wasm-bindgen and tsify
  - `server-core` and `phase-server`: Axum WebSocket server
  - `feed-scraper`: MTGGoldfish metagame
- **State:** pure reducers, `apply(state, action) -> ActionResult`. Immutable state uses persistent data structures: **`im`** (`im::Vector`), not rpds as first reported (verified in source, M1). Serde and tsify produce TypeScript discriminated unions.
- **Cards:** "34,300+ cards parsed from MTGJSON". "Thousands of cards are still unimplemented." Coverage badges come from a `cargo coverage` card-support report.
- **Parser** (DeepWiki summary; the nom dependency and per-card coverage output were verified in M1, see [[phase.rs Evaluation]]):
  - built on `nom` combinators, avoiding regex
  - pipeline: clean → classify line type → route → specialized sub-parsers → lower to `AbilityDefinition`
  - sub-parsers cover keywords (`oracle_keyword.rs`), costs and casting (`oracle_cost.rs`, `oracle_casting.rs`), and targets and filters (`TargetFilter`)
  - an `oracle_gen` binary emits `card-data.json`
- **A known failure mode:** PR #6526 describes a trigger that fired correctly while its token-creation effect **silently lowered to unimplemented**.
- **AI-contributor workflow:** `docs/AI-CONTRIBUTOR.md`, a `CLAUDE.md`, and an `$engine-implementer` skill. The project uses LLMs to implement cards.

## Relevance to us
| Aspect | Fit | Note |
|---|---|---|
| License | ✅ | Identical dual license. |
| Language and state model | ✅/❓ | Persistent state gives cheap clones, but mutation may be slower. Benchmark it. |
| Oracle parser | ⭐ | The most valuable reusable asset. It informs [[ADR-0007 Card Definition Approach]]. |
| Decision model | ❓ | We need every CR choice exposed and factored decisions. Verify. |
| Hidden information | ❓ | It supports hidden info in multiplayer. Check whether the AI respects views. |
| Simulation throughput | ❓ | No published benchmarks. |
| Goals | ⚠️ | Client-first vs. our simulation- and agent-first focus. |

## Evaluation plan
See [[Option - Build on phase.rs]] and [[ADR-0012 Relationship to phase.rs]]. This happens in M1.

## Sources
- README: https://github.com/phase-rs/phase (and raw README)
- DeepWiki overview and parser pages: https://deepwiki.com/phase-rs/phase, https://deepwiki.com/phase-rs/phase/3-oracle-text-parser
- PR #6526: https://github.com/phase-rs/phase/pull/6526
