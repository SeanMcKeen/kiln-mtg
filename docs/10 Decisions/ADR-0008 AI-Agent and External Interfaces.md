---
type: adr
status: accepted
tags: [adr, interfaces, agents]
updated: 2026-10-27
---

# ADR-0008: AI-Agent and External Interfaces

## Context
The user asked for the best methods to be researched. See [[AI-Agent Integration Research]].

## Decision
**One protocol (`kiln-protocol` serde types plus exported JSON Schemas) with several thin front-ends:**
1. **Rust library API**: the canonical boundary ([[Library API]]).
2. **CLI with `--json`** (M7): the universal agent channel, best for coding agents ([[CLI & REPL]]).
3. **MCP server** (M8, `rmcp`, stdio and Streamable HTTP): ~9 workflow-shaped tools, async jobs, concise/detailed responses, pagination, plus an opt-in live-play toolset ([[MCP Server Tool Catalog]]).
4. **Agent docs**: AGENT_GUIDE, llms.txt, a Claude Code skill, schemas, example specs ([[Agent-Facing Docs Plan]]).
5. **Python package** (M12) and **WASM** (M12).
6. **No HTTP job service for now.** MCP over HTTP covers remote use.

## Consequences
All interfaces stay behaviourally identical because logic lives in `kiln-sim` and `kiln-engine`. Agent evals become part of release QA.

**Accept by:** M7 (protocol and CLI), confirmed at M8 (MCP).

## M7–M8 evidence (2026-10-27)
- One protocol: `kiln-protocol` serde types with committed JSON Schemas (spec, report, concise report, game record, run manifest).
- CLI with `--json` (M7) and the MCP server (M8, `rmcp`, stdio) are thin front-ends over `kiln-sim`. Comparison and inspection were moved into `kiln-sim` so both share them.
- Agent docs: `docs/agents/AGENT_GUIDE.md`, `llms.txt`, the `kiln-sim` Claude Code skill, example specs and playbooks.
- Deviations: Streamable HTTP is deferred (Q40). The live-model eval suite is written and waits for the user's go-ahead.

**Status: accepted (2026-10-27)** on this evidence, under the user's standing approval of researched recommendations.
