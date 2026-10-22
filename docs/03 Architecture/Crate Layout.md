---
type: architecture
status: proposed
tags: [architecture, rust, crates]
updated: 2026-10-22
---

# Crate Layout (proposed)

This is a single Cargo workspace. The arrows show dependencies, and lower layers never depend on higher ones.

```
kiln-types       ← primitive types: ids, colors, mana symbols, costs, numbers, CR refs
  ▲
kiln-cards       ← card IR (effects, abilities, filters, costs), compiler front-end(s), coverage
  ▲     ▲
  │   kiln-data  ← Scryfall/MTGJSON ingestion → versioned binary card DB (build tool + loader)
  │
kiln-engine      ← GameState, rules procedures, decisions, events, views, replay
  ▲
kiln-policy      ← Policy trait, PlayerView helpers, baselines (random, greedy), composition
  ▲       ▲              ▲
  │  kiln-playbook   kiln-search     ← Rhai (TBD) sandbox  |  ISMCTS search player
  │       ▲              ▲
kiln-sim         ← experiment specs, seating, batch runner, metrics, reports, deck import
  ▲
kiln-protocol    ← serde JSON schema for decisions/choices/views/specs/reports (+ JSON Schema export)
  ▲
kiln-mtg  kiln-mcp  kiln-py  kiln-wasm   ← interface binaries/bindings (thin)

kiln-testkit     ← scenario file loader, assertion helpers, golden replay runner, fuzz harness
                   (dev-dependency of engine/cards/sim)
```

| Crate | Kind | Key dependencies (candidates) | Milestone |
|---|---|---|---|
| `kiln-types` | lib | `serde`, `smallvec` | M1 |
| `kiln-data` | lib + bin | `serde_json`, `reqwest` (blocking) or `ureq`, `flate2`, `bincode`/`rkyv` | M1 |
| `kiln-cards` | lib | `nom` or `winnow` (if parser front-end), `serde` | M3 |
| `kiln-engine` | lib | `rand_chacha`, `slotmap` or `rpds`/`im` (benchmark), `smallvec` | M2 |
| `kiln-policy` | lib | — | M2 |
| `kiln-sim` | lib | `rayon`, `arrow`/`parquet` (optional feature), `statrs` | M7 |
| `kiln-playbook` | lib | `rhai` (sandboxed) | M8 |
| `kiln-search` | lib | — | M9 |
| `kiln-protocol` | lib | `serde`, `schemars` | M7 |
| `kiln-mtg` | bin | `clap` | M2 → M7 |
| `kiln-mcp` | bin | `rmcp` (official MCP Rust SDK), `tokio` | M8 |
| `kiln-py` | cdylib | `pyo3`, `maturin` | M12 |
| `kiln-wasm` | cdylib | `wasm-bindgen`, `tsify` | M12 |
| `kiln-testkit` | lib (dev) | `proptest`, `insta` | M1 |

**Naming:** every crate uses the `kiln-` prefix. `kiln-core` and `kiln-cli` were already taken on crates.io, so the primitives crate is `kiln-types` and the CLI crate is `kiln-mtg`, which is also the crate users install (`cargo install kiln-mtg` gives the `kiln` binary). See [[ADR-0013 Project Name]].

## Rules
- **The engine doesn't know specific card names.** Card behaviour arrives only through the IR. The one exception is rules that name cards: the CR names a few, and some cards reference others by name, which goes through IR filters.
- **Policies use only `kiln-engine`'s public API.** That keeps the API honest, because bots, the LLM bridge, and third-party users all hold the same capabilities.
- **Interfaces are thin.** Business logic lives in `kiln-sim` and `kiln-engine` so that MCP, CLI, Python and WASM behave identically.
- **Feature flags** cover heavy optional dependencies (parquet, network ingestion).

## Open questions
- `rhai` vs `mlua` vs `wasmtime` for playbooks: see [[B - Playbooks]].
- Persistent (`rpds`/`im`) vs arena-clone state: see [[Performance & Cheap Cloning]].
- Whether to vendor or depend on any of phase.rs ([[ADR-0012 Relationship to phase.rs]]).

## Related
[[Architecture Overview]] · [[Rust Ecosystem]]
