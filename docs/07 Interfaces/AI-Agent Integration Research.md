---
type: research
status: draft
tags: [interfaces, agents, mcp, research]
updated: 2026-10-20
---

# AI-Agent Integration Research

**Question (from the user):** "What's the best way for someone's AI to drive this tool?" The answer feeds [[ADR-0008 AI-Agent and External Interfaces]].

## What agents need for the headline use case
1. **Import decks** and find out immediately whether every card is supported.
2. **Describe an experiment** declaratively: decks, seats, policies, games, flags.
3. **Run long jobs** of minutes to an hour, with progress, without holding a connection open or blowing up context.
4. **Get compact results first**, then drill into details on demand.
5. **Inspect individual games**, either to explain a result or to debug a policy.
6. (Optional) **Pilot a seat live** ([[C - LLM Live Decisions]]).
7. (Judge) **Ask rules questions** through scenario setup and stepping.

## Channels compared
| Channel | Who can use it | Strengths | Weaknesses |
|---|---|---|---|
| **CLI + JSON output** | Any agent with a shell (Claude Code, Codex, Cursor, scripts, CI) | Universal and scriptable. Agents can write spec *files* and pipe output through `jq`. Zero protocol overhead. Composable with code execution. | Not available to chat-only agents. Long jobs need background-process handling. |
| **MCP server** | Claude Desktop/claude.ai, IDEs, any MCP client | First-class tool calling with schemas and descriptions. Works in chat apps. Supports long-running patterns (job IDs). | Tool definitions cost context. Too many tools degrade agent performance. Responses must be token-budgeted. |
| **Python package** | Notebook users, data scientists, RL researchers, agents with a Python sandbox | pandas/Parquet analysis, custom Python policies, gym-style env | Build complexity (PyO3/maturin wheels). |
| **HTTP/JSON service** | Remote or multi-user setups, web apps | Job queues, shared runs | Ops burden, auth. Not needed for local-first use. |

## Lessons from Anthropic's tool-design guidance
From [Writing effective tools for agents](https://www.anthropic.com/engineering/writing-tools-for-agents):
- **Few, workflow-shaped tools** beat wrapping every endpoint. Consolidate multi-step operations. Each tool needs a clear, distinct purpose.
- **Namespace** tools (e.g. `mtg_sim_run`, `mtg_deck_import`).
- **Return meaningful context:** resolve opaque IDs into names (card names, player seats, deck names) and keep the IDs only where a later call needs them.
- **`response_format: concise | detailed`.** In their example, concise responses used about ⅓ of the tokens.
- **Paginate, filter and truncate** with sensible defaults. Claude Code caps tool responses at 25k tokens by default. When truncating, tell the agent how to narrow the query.
- **Actionable errors:** say what's wrong and show an example of valid input.
- Write descriptions like onboarding a new hire. Use unambiguous parameter names (`deck_id` not `deck`). Use strict schemas.
- **Evaluate tools with realistic multi-call tasks** and read the transcripts.

## Lessons from prior LLM-plays-MTG projects
From [[LLMs Playing MTG]]:
- **MageBench** wired LLMs to XMage through MCP tools (`pass_priority` → read choices → `choose_action`), with short stable object IDs like "p3" and pre-filtered castable cards. Costs were **≈ $1+ per game**. Failure modes included hallucinated card text, harness bugs around triggers, and weak strategic play.
  - **Lessons for us:**
    - always include Oracle text for visible cards in decision payloads
    - pre-filter options (we do this by construction)
    - use short, stable references
    - keep live LLM play to spot checks
- **MTG Bench** let the LLM track state itself, which produced frequent errors. That confirms the engine must own all state.

## Recommendation (→ ADR-0008)
**One protocol, several thin front-ends:**
1. **`mtg-protocol`:** serde types and an exported **JSON Schema** for decks, experiment specs, decisions, choices, views and reports. Every front-end speaks this.
2. **CLI first (M7):** `mtg sim run spec.yaml --json`, `mtg sim report run-id --format concise`, `mtg deck check`, `mtg game replay`. It's the simplest channel and the best one for coding agents.
3. **MCP server (M8)** with **~8–10 workflow tools** ([[MCP Server Tool Catalog]]):
   - built on the official Rust SDK `rmcp`, with stdio plus Streamable HTTP transports
   - async job pattern for runs
   - concise/detailed reports with pagination
4. **Agent docs (M8):** an agent guide, `llms.txt`, and a Claude Code skill ([[Agent-Facing Docs Plan]]).
5. **Python package (M12)** for analysis and custom policies. **WASM (M12)** for a browser judge tool.
6. **HTTP service: not planned.** It can be added later over the same protocol, since the MCP Streamable HTTP transport already covers remote use.

## Open questions
- Should the MCP server expose a single "run experiment from spec text" tool, or structured parameters? **Leaning: both.** A spec-text tool for power users, plus a structured `mtg_sim_run` whose schema mirrors the spec.
- Where run artifacts live (a local `runs/` directory vs. a configurable store) and how an MCP client in claude.ai reaches large outputs. Probably summary inline, with artifacts on disk under a path.

## Sources
- Anthropic, "Writing effective tools for agents — with agents": https://www.anthropic.com/engineering/writing-tools-for-agents
- Official MCP Rust SDK (`rmcp`), README on docs.rs: https://docs.rs/crate/rmcp/latest
- MageBench HN discussion: https://hn.svelte.dev/item/47049227
- MageBench leaderboard listing: https://benchmarklist.com/benchmarks/mage_bench_season1/
