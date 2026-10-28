---
type: milestone
status: active
milestone: M10
tags: [milestone, llm, judge]
updated: 2026-10-28
---

# M10: LLM Bridge & Judge

## Checklist
- [ ] Live-play MCP toolset and a built-in LLM adapter ([[C - LLM Live Decisions]])
- [ ] Escalation composition: LLM at key moments
- [ ] Rationale-annotated replays
- [ ] Judge REPL: scenario load, step, `why-not`, `why`, branch ([[Judge Tool UX]])
- [ ] `kiln_rules_scenario` MCP tool

## Exit criteria
US-5 and US-6 work.

## Starting point (2026-10-28)
- `kiln_rules_scenario` exists (M8): it runs a scenario board and returns the event log and the result. Every engine event and decision carries a CR reference (`Event::cr`, `PendingDecision::cr`).
- Policies: random, greedy, passive, playbook, compose, search (M9). `kind: llm` is parsed and rejected. `SearchPolicy::last_search()` holds the last decision's options with visits and values.
- Replays (`kiln-replay/1`) log choices only. Nothing explains why an action is unavailable.

## Plan
### Design decisions
| # | Decision | Why |
|---|---|---|
| L1 | **`Game::why_not(player, action)`** returns the reasons an action isn't offered, each with a CR rule and the card responsible: zone, priority, split second, "can't cast" effects (naming the permanent), timing, modes, Aura targets, spell targets, costs; for lands one per turn and timing; for abilities controller, restrictions, timing, once per turn, costs. It mirrors the engine's own checks, so it never invents a rule (FR-E10). | [[Judge Tool UX]]: explanations come from the engine, never from a policy or an LLM. |
| L2 | **Key decisions** (`kiln_policy::is_key_decision`): mulligans, priority with a non-mana action, targets, attacks, blocks, card choices, modes, names, players, "may" choices. Shared by search (M9), live play and the LLM adapter. | One definition of "worth asking", per [[C - LLM Live Decisions]] ("auto-pass trivial decisions"). |
| L3 | **Judge REPL**: `kiln judge <scenario.yaml>` runs the scenario script, then steps interactively: a number or label chooses; `why-not <label or card>`; `why <event #>` (the event's CR and the cast, trigger or resolution that caused it); `branch` / `back` keep a stack of game copies; `events`, `view`, `quit`. `--json` and a non-interactive `--command` list serve agents. `kiln_rules_scenario` gains `why_not: [labels]`. | US-5. |
| L4 | **Live-play MCP toolset** behind `kiln mcp serve --live`: `kiln_live_start` (a kiln-spec/1 pod naming the caller's deck, a seed, `ask: key or all`), `kiln_live_decide` (option index or label, optional rationale), `kiln_live_view` (concise or detailed). Other seats play their spec policies; the caller's trivial decisions go to its fallback (greedy). Each payload has the decision context with its CR, the options, a concise board, and **Oracle text for every card it names**. Finished games are saved as annotated replays. | US-6 through any MCP client (Claude Code), with no API spend. The toolset is opt-in so it doesn't cost context when unused. |
| L5 | **Rationales**: `Policy::rationale()` (default none). The replay gets optional per-decision annotations: an LLM's or live caller's reason, search's top options with visits and values. `kiln game replay --annotate` prints them. | Rationale-annotated replays for humans ([[C - LLM Live Decisions]]). |
| L6 | **Built-in LLM adapter**: `{ kind: llm, model, ask: key, max_calls }` calls the Anthropic Messages API (key from `ANTHROPIC_API_KEY`) with the same payload, and parses a reply naming an option. It declines non-key decisions (`ask: key`), so `compose: [llm, search]` is "LLM at key moments" with no new machinery. Replies that don't name an option fall through and are counted. Runs are marked not reproducible; replays are, since choices are logged. Tested against a local mock server. | US-6 with an API key. Live API calls cost money, so they wait for the user's go-ahead. |
| L7 | **US-6 demonstration in chat**: the session pilots a seat through the live tools, like the M8 eval. | Same reason as the M8 eval: no API spend. |

### Phases
| Phase | Scope | Check |
|---|---|---|
| 10.1 | `why_not`, `is_key_decision`, replay annotations | Unit tests per reason, CR numbers checked |
| 10.2 | Judge REPL and `kiln_rules_scenario` `why_not` | US-5 (Thassa's Oracle with Notion Thief) end to end |
| 10.3 | Live toolset, payloads with Oracle text, annotated replays | MCP smoke test plays a game |
| 10.4 | LLM adapter, escalation through compose, spec and report | Mock-server tests; spec validation |
| 10.5 | In-chat US-6 game, docs, merge | US-6 transcript and annotated replay |

## Related
[[C - LLM Live Decisions]] · [[Judge Tool UX]] · [[MCP Server Tool Catalog]] · [[CLI & REPL]]
