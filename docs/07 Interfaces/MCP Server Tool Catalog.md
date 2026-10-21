---
type: design
status: draft
tags: [interfaces, mcp, agents]
updated: 2026-10-20
---

# MCP Server Tool Catalog (draft)

These are workflow-shaped and namespaced `mtg_*` tools ([[AI-Agent Integration Research]]). The target is **≤ 10 tools** for the core server. Live-play tools sit behind a separate toolset flag so they don't spend context when unused.

## Core toolset
| Tool | Purpose | Key params | Returns (concise) |
|---|---|---|---|
| `mtg_deck_import` | Parse and validate a decklist | `source` (text \| url \| path), `format` (default commander), `name?` | `deck_id`, name, commander(s), card count, **legality issues**, **unsupported/unverified cards**, color identity |
| `mtg_card_lookup` | Look up cards and their engine support status | `names[]` or `query`, `response_format` | Oracle text, types, **coverage status**, known issues |
| `mtg_sim_run` | Start an experiment (async) | `spec` (structured, mirrors [[Experiment Spec]]) **or** `spec_text` (YAML) | `run_id`, validated spec echo, estimated duration, warnings |
| `mtg_sim_status` | Progress of a run | `run_id` | % done, games done/total, ETA, interim win rates (concise) |
| `mtg_sim_report` | Results | `run_id`, `sections[]` (win_rates, game_shape, per_card, mulligans, …), `response_format`, `page`, `filter` | ≤ ~2k tokens concise. Detailed is paginated. Includes artifact paths |
| `mtg_sim_compare` | A/B or multi-run comparison | `run_ids[]`, `metric` | Deltas with CIs |
| `mtg_game_inspect` | Look at one simulated game | `run_id`, `game_index` or `query` (e.g. "games where deck X lost before turn 4"), `turns?`, `response_format` | Narrative log with card names, key decisions, result |
| `mtg_rules_scenario` | Judge tool: set up a board and step through it | `scenario` (YAML, [[Scenario File Format]]), `actions[]?`, `question?` | Event log with CR citations, final state, legality explanations |
| `mtg_playbook_validate` | Lint and dry-run a playbook | `deck_id`, `playbook_text` | Errors with line numbers, a sample of decisions it would make |

## Live-play toolset (opt-in, [[C - LLM Live Decisions]])
| Tool | Purpose |
|---|---|
| `mtg_live_start` | Start a game with one or more seats controlled by the caller. Returns `game_id` and the first pending decision |
| `mtg_live_decide` | Submit a choice and get the next pending decision for a caller-controlled seat (other seats auto-play with their policies) |
| `mtg_live_view` | The current redacted view, with concise and detailed modes |

## Conventions
- **Names over IDs:** decks by name, cards by name, seats as "Seat 2 (Najeela)". Short stable refs (e.g. `o17`) appear only where a follow-up call needs them.
- **Every response** includes the `card_db` and `engine` versions in the detailed format only. Concise omits them unless they changed.
- **Errors** are actionable: `"Unknown card 'Thassas Oracle' — did you mean 'Thassa's Oracle'?"`, `"games must be ≤ 1,000,000; got 5e9"`.
- **Long outputs** are written to disk as artifacts (Parquet/JSONL). The tool response gives a path and a summary.
- **Safety:** runs can be cancelled (`mtg_sim_status` with `cancel: true`). There are resource caps (max concurrent runs, max games).

## Evaluation (M8)
Write about 15 realistic agent tasks (US-1 through US-8 variants), run them with Claude, and measure success, tool calls, tokens and errors. Iterate on descriptions.

## Related
[[AI-Agent Integration Research]] · [[CLI & REPL]] · [[Output Formats & Report Schema]]
