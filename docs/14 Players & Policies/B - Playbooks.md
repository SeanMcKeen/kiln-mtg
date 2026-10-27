---
type: design
status: draft
tags: [policies, playbooks, agents, scripting]
updated: 2026-10-27
---

# B: AI-Authored Playbooks

## Concept
A **playbook** is a per-deck script that encodes how to pilot that deck. A user's AI writes it from the decklist ("read this list, write the playbook"). It runs **sandboxed, at full speed**, inside the sim.

What a playbook expresses:
- **Mulligan rules:** "keep 7 if ≥2 mana sources and (a fast mana piece or a tutor) and a blue source".
- **Lines and combos:** "if Thassa's Oracle + Demonic Consultation are both castable with protection, go."
- **Priorities:** "counter any tutor or win attempt; let removal through unless it targets Kinnan".
- **Targeting preferences:** "remove the player closest to winning; prefer hitting stax pieces".
- **Tags:** "a win attempt", "a stax piece", which feed [[Metrics Catalog]] (win type, interaction).
- **Delegation:** "for anything not covered, ask the fallback (search or greedy)."

## Sandboxed language: candidates
| | **Rhai** | **Lua (mlua)** | **WASM (wasmtime)** |
|---|---|---|---|
| Embedding in Rust | Native, pure Rust | C library binding | Mature runtime |
| Sandboxing | No built-in I/O. Every effect is a host-registered function. Limits on **operations** (`set_max_operations`), call depth, string, array and map sizes | Needs care (strip stdlib). Instruction-count hooks | Strong isolation. Fuel metering |
| LLM authorability | Good (Rust/JS-like syntax), but less training data | **Excellent** (very common language) | Poor directly. Needs a source language and compiler |
| Determinism | Good (no I/O, no time) | Good, if stdlib stripped | Good |
| Speed | Moderate (AST interpreter) | Fast (LuaJIT not in mlua by default; PUC Lua OK) | Fastest |

**Decision (M8, 2026-10-27): Rhai.** Reasons:
- Pure Rust, so the WASM build (M12) stays simple; Lua needs a C library.
- The sandbox is built in: no I/O, plus limits on operations, call depth and sizes. Kiln builds Rhai without time functions or file modules, so playbooks are deterministic.
- Rhai's maps are sorted, so iteration order is stable.
- LLMs write its JS-like syntax readily.

A declarative rule layer can still sit on top later. Details: [[M8 Agent Integration & Playbooks]] (plan P1–P3).

### As implemented
- A playbook defines functions named after decision kinds (`mulligan`, `priority`, `choose_target`, `declare_attacker`, `bottom_card`, …) or a catch-all `decide`, each taking `(view, decision)`. It returns an option index or label, or `()` to defer.
- `decision` has `kind`, `options` (labels such as "Cast Sol Ring", "Play Island", "Keep") and `count`. `view` is read lazily from the redacted player view: `me`, `turn`, `my_turn`, `step`, `players[i]` (life, hand_count, library_count, …), `hand`, `battlefield`, `stack`, `graveyard`, `graveyards`, `command`. Objects carry `name`, `types`, `mana_value`, `land`, `creature`, `controller`, `tapped`, `power`, `toughness` and more.
- Helpers: `find_option(options, text)`, `has_option(options, text)`, `names(cards)`, `tag(name)`.
- Limits: 200,000 operations per call, plus call-depth and size limits. A runtime error, a limit hit or an invalid answer defers and is counted (`playbook:errors`).
- Example: `examples/playbooks/generic-cedh.rhai`.

### Alternative: declarative playbooks
A YAML rule list (conditions → preferred actions) instead of a script. It's safer, easier to validate, and LLM-friendly, but less expressive. **Possible approach:** a YAML rule list for 90% of cases, with an optional script block.

## Host API exposed to playbooks (sketch)
`view.my_hand()`, `view.castable()`, `view.opponents()`, `view.stack()`, `card.name`, `card.tags`, `decision.kind`, `decision.options`, `choose(i)`, `defer()` (to the fallback), `tag("win_attempt")`, plus helpers like `can_assemble(["Thassa's Oracle","Demonic Consultation"])`.

## Validation workflow (for agents)
`kiln_playbook_validate` checks syntax, runs 50 games against passive opponents, and reports:
- errors with line numbers
- how often each rule fired
- decisions deferred to the fallback

## Sources
- Rhai safety limits (max operations): https://rhai.rs/book/safety/max-operations.html
- Rhai crate docs (limits on call levels, strings, arrays, maps): https://docs.rs/rhai/

## Related
[[Policy Composition]] · [[D - Search AI]] · [[MCP Server Tool Catalog]]
