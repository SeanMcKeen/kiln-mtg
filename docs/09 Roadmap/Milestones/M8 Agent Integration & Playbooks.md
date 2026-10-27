---
type: milestone
status: in-progress
milestone: M8
tags: [milestone, agents]
updated: 2026-10-27
---

# M8: Agent Integration & Playbooks

## Checklist
- [ ] MCP server (`rmcp`, stdio and Streamable HTTP), core toolset ([[MCP Server Tool Catalog]]) → accept ADR-0008
- [ ] Async jobs, concise/detailed responses, pagination, actionable errors
- [ ] Playbook language decision (Rhai vs. Lua vs. declarative YAML) and runtime with limits ([[B - Playbooks]])
- [ ] `kiln_playbook_validate`, plus example playbooks for the four test decks
- [ ] Policy composition ([[Policy Composition]])
- [ ] Agent docs: AGENT_GUIDE, llms.txt, Claude Code skill, example specs ([[Agent-Facing Docs Plan]])
- [ ] Agent eval suite: about 15 tasks, measure and iterate

## Exit criteria
Claude completes US-1 end to end via MCP with no human help: it writes playbooks, runs the experiment, and interprets the CIs correctly.

## Starting point (2026-10-27)
- M7 delivered `kiln-spec/1`, the experiment runner, `kiln-report/1` and the `kiln sim` / `kiln game` CLI. Policies are random, greedy and passive; `playbook`, `search` and `llm` kinds are parsed but rejected.
- `kiln-playbook` and `kiln-mcp` are empty stubs.

## Plan
### Design decisions
| # | Decision | Why |
|---|---|---|
| P1 | **Playbooks are Rhai scripts** (Q30). A playbook defines optional functions (`mulligan`, `priority`, `target`, `attack`, `block`, `choose`, …) that get a read-only view and the decision, and return an option index, or nothing to **defer** to the fallback. | Pure Rust (keeps the M12 WASM build simple; Lua needs a C library), a built-in sandbox (no I/O; limits on operations, call depth, string/array/map sizes), sorted maps (deterministic), and JS-like syntax LLMs write well. A declarative rule layer can sit on top later. |
| P2 | **Limits per decision**: max operations, call depth and sizes; a playbook that exceeds them, errors at run time or returns an invalid index **defers** for that decision and the error is counted and reported (never a crash, never an illegal choice). | Legality by construction; an AI-written script can't break a run. |
| P3 | **Host API**: the decision kind and options as labels; the viewer's hand, battlefield, graveyard, life, mana, opponents' public state, the stack; helpers `castable(name)`, `option_named(text)`, `has(name)`, `count(zone, filter)`; `tag(name)` marks why a choice was made (rule-fire counts). Everything comes from `PlayerView`, so playbooks can't see hidden information. | The view auditor's guarantees carry over. |
| P4 | **Composition** in `kiln-policy`: `Fallback(a, b)` and `ByDecisionKind`. Spec kinds: `playbook { playbook, fallback }` (fallback default greedy) and `compose { chain: [...] }`. The report states each seat's recipe and how often each component answered. | Policy Composition. |
| P5 | **`kiln playbook check <deck> <playbook>`** (and the MCP tool): compile errors with line numbers, then a dry run of N goldfish games reporting rule-fire counts, deferrals and runtime errors. | B - Playbooks, validation workflow. |
| P6 | **MCP server** in `kiln-mcp` with `rmcp` (stdio, plus Streamable HTTP), `kiln mcp serve`. Core tools: `kiln_deck_import`, `kiln_card_lookup`, `kiln_sim_run` (async: returns a run id), `kiln_sim_status` (with cancel), `kiln_sim_report` (sections, concise/detailed, pagination), `kiln_sim_compare`, `kiln_game_inspect`, `kiln_rules_scenario`, `kiln_playbook_validate`. Logic stays in `kiln-sim`; tools are thin. Errors are actionable ("did you mean"). | ADR-0008, MCP Server Tool Catalog. |
| P7 | **Agent docs**: `docs/agents/AGENT_GUIDE.md`, `llms.txt`, a Claude Code skill (`.claude/skills/kiln-sim/`), the JSON Schemas, and example specs and playbooks. | Agent-Facing Docs Plan. |
| P8 | **Agent eval suite**: about 15 task definitions with success checks. Running them with a live model spends API usage, so that step waits for the user's go-ahead. | Exit criterion. |

### Phases
| Phase | Content | Evidence |
|---|---|---|
| 8.1 | Q30 decision note; `kiln-playbook` (Rhai runtime, limits, host API); `PlaybookPolicy`, `Fallback`, `ByDecisionKind`; spec kinds `playbook` and `compose`. | Unit tests; a playbook driving games |
| 8.2 | `kiln playbook check`; example playbooks for the four test decks; recipe and component counts in reports. | Playbooks vs. greedy in a pod |
| 8.3 | MCP server with the core tools, async runs, pagination, errors; `kiln mcp serve`. | Protocol tests over stdio |
| 8.4 | Agent docs, skill, llms.txt; agent eval task suite. | Docs review |
| 8.5 | Eval runs with a live model (after the user's go-ahead), ADR-0008 acceptance → merge. | Exit criterion |
