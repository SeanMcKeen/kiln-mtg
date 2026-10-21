---
type: architecture
status: accepted-in-part
tags: [architecture, rust, crates]
updated: 2026-10-20
---

# Crate Layout (proposed)

This is a single Cargo workspace. The arrows show dependencies, and lower layers never depend on higher ones.

```
mtg-core        ← primitive types: ids, colors, mana symbols, costs, numbers, CR refs
  ▲
mtg-cards       ← card IR (effects, abilities, filters, costs), compiler front-end(s), coverage
  ▲     ▲
  │   mtg-data  ← Scryfall/MTGJSON ingestion → versioned binary card DB (build tool + loader)
  │
mtg-engine      ← GameState, rules procedures, decisions, events, views, replay
  ▲
mtg-policy      ← Policy trait, PlayerView helpers, baselines (random, greedy), composition
  ▲       ▲          ▲
  │  mtg-playbook   mtg-search      ← Rhai (TBD) sandbox  |  ISMCTS search player
  │       ▲          ▲
mtg-sim         ← experiment specs, seating, batch runner, metrics, reports, deck import
  ▲
mtg-protocol    ← serde JSON schema for decisions/choices/views/specs/reports (+ JSON Schema export)
  ▲
mtg-cli  mtg-mcp  mtg-py  mtg-wasm        ← interface binaries/bindings (thin)

mtg-testkit     ← scenario file loader, assertion helpers, golden replay runner, fuzz harness
                  (dev-dependency of engine/cards/sim)
```

| Crate | Kind | Key dependencies (candidates) | Milestone |
|---|---|---|---|
| `mtg-core` | lib | `serde`, `smallvec` | M1 |
| `mtg-data` | lib + bin | `serde_json`, `reqwest` (blocking) or `ureq`, `flate2`, `bincode`/`rkyv` | M1 |
| `mtg-cards` | lib | `nom` or `winnow` (if parser front-end), `serde` | M3 |
| `mtg-engine` | lib | `rand_chacha`, `slotmap` or `rpds`/`im` (benchmark), `smallvec` | M2 |
| `mtg-policy` | lib | — | M2 |
| `mtg-sim` | lib | `rayon`, `arrow`/`parquet` (optional feature), `statrs` | M7 |
| `mtg-playbook` | lib | `rhai` (sandboxed) | M8 |
| `mtg-search` | lib | — | M9 |
| `mtg-protocol` | lib | `serde`, `schemars` | M7 |
| `mtg-cli` | bin | `clap` | M2 → M7 |
| `mtg-mcp` | bin | `rmcp` (official MCP Rust SDK), `tokio` | M8 |
| `mtg-py` | cdylib | `pyo3`, `maturin` | M12 |
| `mtg-wasm` | cdylib | `wasm-bindgen`, `tsify` | M12 |
| `mtg-testkit` | lib (dev) | `proptest`, `insta` | M1 |

## As built (M1, 2026-10-20)
All crates exist under `crates/`. The ones M1 doesn't implement are stubs whose crate docs point at their design note and milestone.

| Crate | M1 state | Dependencies chosen |
|---|---|---|
| `mtg-core` | ids, `Color`/`Colors`, `ManaSymbol`/`ManaCost`, `TypeLine`, `PrintedStat`, `CrRef` | `serde`, `smallvec`, `uuid`, `thiserror` |
| `mtg-cards` | only the coverage model (`Coverage`, `Provenance`, `CardDefinition` without IR) | `serde` |
| `mtg-data` | Scryfall fetch, normalization, binary DB, name index | **`postcard`** for the DB (bincode is unmaintained; rkyv deferred until load time matters, since ~210 ms already beats NFR-P6), **`ureq` 3** (blocking, behind the `fetch` feature), `flate2`, `sha2`, `strsim`, `unicode-normalization` |
| `mtg-sim` | deck import and Commander validation only | `csv`, `sha2` |
| `mtg-testkit` | CR rule tree and citation checker | `regex` |
| `mtg-cli` | `mtg data …`, `mtg deck check`, `mtg dev cr-check` | `clap` (derive, env), `anyhow` |

Workspace conventions:
- edition 2024, toolchain pinned in `rust-toolchain.toml`
- `unsafe_code = "forbid"`, a small set of clippy lints, and CI runs `-D warnings`

Deck import landed in `mtg-sim` as this note says. If it stays CLI-only through M6, consider moving it to `mtg-data` so the CLI doesn't pull in simulation code.

## Rules
- **The engine doesn't know specific card names.** Card behaviour arrives only through the IR. The one exception is rules that name cards: the CR names a few, and some cards reference others by name, which goes through IR filters.
- **Policies use only `mtg-engine`'s public API.** That keeps the API honest, because bots, the LLM bridge, and third-party users all hold the same capabilities.
- **Interfaces are thin.** Business logic lives in `mtg-sim` and `mtg-engine` so that MCP, CLI, Python and WASM behave identically.
- **Feature flags** cover heavy optional dependencies (parquet, network ingestion).

## Open questions
- `rhai` vs `mlua` vs `wasmtime` for playbooks: see [[B - Playbooks]].
- Persistent (`rpds`/`im`) vs arena-clone state: see [[Performance & Cheap Cloning]].
- Whether to vendor or depend on any of phase.rs ([[ADR-0012 Relationship to phase.rs]]).

## Related
[[Architecture Overview]] · [[Rust Ecosystem]]
