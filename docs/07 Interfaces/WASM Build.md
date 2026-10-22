---
type: design
status: draft
tags: [interfaces, wasm]
updated: 2026-10-22
---

# WASM Build (M12)

- Published to npm as `kiln-mtg` (the bare `kiln` name is taken; see [[ADR-0013 Project Name]]), built from the `kiln-wasm` crate.
- Target `wasm32-unknown-unknown` with `wasm-bindgen` + `tsify` for TypeScript types (as phase.rs does).
- **Main use:** a browser **rules-judge** tool and small in-browser simulations (single-threaded, or using Web Workers).
- Constraints: no threads by default (rayon needs special setup), the card DB must be loaded as a fetched binary (it can't be bundled for legal reasons), and memory limits apply.
- Engine crates must avoid OS-specific dependencies (filesystem, threads) in core paths. Gate those behind features.

## Related
[[Library API]] · [[Judge Tool UX]]
