---
type: design
status: draft
tags: [interfaces, api, rust]
updated: 2026-10-20
---

# Library API (Rust)

The **public API is the product boundary**. Policies, the sim, and all interfaces use only it ([[Crate Layout]] rules).

## Layers
1. **`mtg-engine` (game level):** `Game::new(setup, card_db, seed)`, `advance()`, `pending()`, `apply(choice)`, `view_for(player)`, `clone()`, `hash()`, `events()`, `replay_log()`.
2. **`mtg-policy`:** the `Policy` trait and composition helpers ([[Policy Interface]]).
3. **`mtg-sim` (experiment level):** `Experiment::from_spec(spec)`, `run(&self, progress: impl Progress) -> RunResult`, `Report::sections(flags)`.

## Stability
- Pre-1.0: breaking changes are allowed but recorded in a CHANGELOG.
- `mtg-protocol` JSON schemas are **versioned separately** (`mtg-replay/1`, `mtg-spec/1`, `mtg-report/1`) so stored files stay readable.

## Error model
- `apply()` with an option that wasn't offered gives `Err(InvalidChoice)`. That's a client bug, never a rules probe.
- Setup errors (illegal deck, unknown card, unsupported card) are typed errors with actionable messages.

## Related
[[Decision & Action Model]] · [[Python Package]] · [[WASM Build]]
