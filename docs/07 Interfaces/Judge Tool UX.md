---
type: design
status: draft
tags: [interfaces, judge]
updated: 2026-10-20
---

# Judge Tool UX

**Goal:** answer "what happens if…" and "why can't I…" questions **exactly**, with CR citations.

## Flow
1. **Describe the scenario** in a [[Scenario File Format]] YAML file, or by asking an agent to write it: players, life, zones, permanents with status and counters, the stack, turn, step, and the priority holder.
2. **Validate:** the engine checks the scenario is a reachable-looking legal state, and warns about impossible states (e.g. two copies of a singleton card in Commander).
3. **Step:** take actions from the legal menu. Every event shows its CR rule.
4. **Explain:**
   - `why-not <action>` reports which rule or effect forbids it (FR-E10)
   - `why <event>` reports the causal chain (event → trigger → resolution)
5. **Branch:** fork the state ("what if P2 responds with Swan Song?") and compare outcomes.

## Explanation quality bar
Every explanation must cite a CR rule or an Oracle ruling. Policies never generate explanations. They come from the engine's event metadata ([[Event System]]). LLMs may *phrase* explanations nicely, but they must not invent rules.

## As implemented (M10)
- `kiln judge <scenario.yaml> [-c <command>]…`: runs the scenario's script, then reads commands (interactive, or `-c` for agents): a number or label chooses; `why-not [P2] <label or card>`; `why <event #>`; `events [n]`; `view`; `branch` / `back`; `result`. The real-card pool is used when the card DB exists.
- `why-not` comes from `Game::why_not`, which walks the engine's own legality checks in order and names the responsible card (e.g. `CR 101.2: Test Abolisher: it says this player can't cast this spell now`); `why` gives the event's rule and the cast, activation, trigger, resolution or combat declaration before it.
- MCP: `kiln_rules_scenario` takes the same `commands`.
- Not yet: validation of impossible states (two copies of a singleton), "why" chains longer than one step, and explanations for non-priority decisions (targets that aren't offered).

## Related
[[CLI & REPL]] · US-5 in [[User Stories]] · [[M10 LLM Bridge & Judge]]
