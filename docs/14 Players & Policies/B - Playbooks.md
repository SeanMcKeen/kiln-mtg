---
type: design
status: draft
tags: [policies, playbooks, agents, scripting]
updated: 2026-10-20
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

**Leaning: Rhai** for safety and pure-Rust simplicity, unless the spike shows LLMs write Lua much more reliably. Then Lua (mlua) with a stripped stdlib. → Decided in [[M8 Agent Integration & Playbooks]].

### Alternative: declarative playbooks
A YAML rule list (conditions → preferred actions) instead of a script. It's safer, easier to validate, and LLM-friendly, but less expressive. **Possible approach:** a YAML rule list for 90% of cases, with an optional script block.

## Host API exposed to playbooks (sketch)
`view.my_hand()`, `view.castable()`, `view.opponents()`, `view.stack()`, `card.name`, `card.tags`, `decision.kind`, `decision.options`, `choose(i)`, `defer()` (to the fallback), `tag("win_attempt")`, plus helpers like `can_assemble(["Thassa's Oracle","Demonic Consultation"])`.

## Validation workflow (for agents)
`mtg_playbook_validate` checks syntax, runs 50 games against passive opponents, and reports:
- errors with line numbers
- how often each rule fired
- decisions deferred to the fallback

## Sources
- Rhai safety limits (max operations): https://rhai.rs/book/safety/max-operations.html
- Rhai crate docs (limits on call levels, strings, arrays, maps): https://docs.rs/rhai/

## Related
[[Policy Composition]] · [[D - Search AI]] · [[MCP Server Tool Catalog]]
