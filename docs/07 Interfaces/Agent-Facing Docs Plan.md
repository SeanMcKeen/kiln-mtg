---
type: plan
status: draft
tags: [interfaces, docs, agents]
updated: 2026-10-20
---

# Agent-Facing Docs Plan (M8)

**Goal (FR-I5):** an AI agent with no prior knowledge can run US-1 end to end using only these docs.

## Deliverables
| Artifact | Audience | Content |
|---|---|---|
| `docs/agents/AGENT_GUIDE.md` | Any agent | Concepts (deck, experiment, policy, metrics), a worked US-1 walkthrough, the spec reference, interpreting CIs, common pitfalls (unverified cards, seat bias, low sample size) |
| `llms.txt` at the repo or site root | LLM crawlers and agents | A short index of the agent docs, following the llms.txt convention |
| Claude Code skill `mtg-sim` | Claude Code users | When to run sims, how to write specs, how to read reports. It wraps the CLI |
| MCP tool descriptions | MCP clients | Written as onboarding docs, evaluated per [[MCP Server Tool Catalog]] |
| JSON Schemas | Agents writing specs | `mtg-spec/1.schema.json` and others, exported from `mtg-protocol` |
| Example specs | Everyone | `examples/specs/cedh-pod.yaml`, `goldfish.yaml`, `ab-test.yaml`, `mulligan-only.yaml` |

## Principles
- Lead with the headline task. Show the minimal spec first.
- Teach statistical humility: always read the CI, and don't over-interpret per-card stats ([[Validity & Caveats]]).
- Keep it versioned alongside the protocol.

## Related
[[AI-Agent Integration Research]] · [[M8 Agent Integration & Playbooks]]
