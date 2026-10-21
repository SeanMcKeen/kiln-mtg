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

## Related
[[CLI & REPL]] · US-5 in [[User Stories]] · [[M10 LLM Bridge & Judge]]
