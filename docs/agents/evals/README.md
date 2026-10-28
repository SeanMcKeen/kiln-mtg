---
type: plan
status: draft
tags: [agents, evals, mcp]
updated: 2026-10-27
---

# Agent eval suite

Fifteen realistic tasks (`tasks.yaml`) for an agent connected to `kiln mcp serve`. They measure whether an agent with no prior knowledge can use Kiln well ([[Agent-Facing Docs Plan]], FR-I5) and feed back into tool descriptions and the [[AGENT_GUIDE]]. They cover:
- the headline deck evaluation (US-1, plus a playbook variant that is the M8 exit criterion),
- goldfish speed (US-2) and opening hands (US-4),
- A/B swaps, the coverage gate, misspellings and error recovery,
- inspection, statistics traps, per-card caveats, rules questions, cancel and resume, and comparison.

## Running
1. Build `kiln` and the card DB. Start each task in a fresh session with a fresh `--workdir`, connected only to the `kiln` server, and give the agent the task's `prompt` verbatim.
2. Record per task: success (every check met), the number of tool calls, tokens, errors from tools, and anything confusing in the tool descriptions.
3. Grade from the transcript and the run directories under the workdir. `tools/mcp-smoke/smoke.py` checks the server itself.

**Running the suite with a live model spends API usage**, so it waits for the user's go-ahead (M8 plan P8).

## Results
| Date | Model | Passed | Notes |
|---|---|---|---|
| — | — | — | not run yet |

## Related
[[M8 Agent Integration & Playbooks]] · [[MCP Server Tool Catalog]]
