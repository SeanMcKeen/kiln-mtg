---
type: research
status: draft
tags: [research, rust]
updated: 2026-10-20
---

# Rust Ecosystem (candidate dependencies)

| Need | Candidates | Notes |
|---|---|---|
| Cheap state cloning | `rpds`, `im` (persistent); `slotmap` / generational arenas + `#[derive(Clone)]` | Benchmark in M2 ([[Performance & Cheap Cloning]]). phase.rs uses rpds. |
| RNG | `rand_chacha` (ChaCha8/12/20) | Seedable and portable. Determinism ([[Determinism & Replay]]). |
| Parallelism | `rayon` | One game per task. |
| Parsing (Oracle, DSL) | `nom`, `winnow`, `chumsky` | phase.rs uses nom. `winnow` has better errors. |
| Serialization | `serde`, `serde_json`, `serde_yaml` (or `serde_yml`), `rkyv` / `bincode` | rkyv for zero-copy card DB loads. |
| JSON Schema | `schemars` | Exports protocol schemas. |
| MCP | **`rmcp`**, the official Rust SDK (tokio). Transports: stdio and **Streamable HTTP** (`StreamableHttpService`) | A 3.x release line is on docs.rs. Pin after checking crates.io. |
| Scripting (playbooks) | **`rhai`** (pure Rust; no I/O by default; `set_max_operations`, call-depth, string/array/map limits); `mlua` (Lua; instruction hooks); `wasmtime` (fuel metering) | [[B - Playbooks]] |
| Python bindings | `pyo3` + `maturin` | [[Python Package]] |
| WASM bindings | `wasm-bindgen`, `tsify` | [[WASM Build]] |
| CLI | `clap` | |
| Tables | `arrow` / `parquet`, `polars` (optional) | Bulk sim output. |
| Stats | `statrs` | Distributions for CIs. |
| Testing | `cargo-nextest`, `proptest`, `insta`, `criterion`, `cargo-fuzz` | [[Testing Strategy]] |

## Sources
- rmcp README (docs.rs): https://docs.rs/crate/rmcp/latest
- Rhai max operations: https://rhai.rs/book/safety/max-operations.html
- Rhai docs: https://docs.rs/rhai/
