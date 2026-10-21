---
type: milestone
status: planned
milestone: M8
tags: [milestone, agents]
updated: 2026-10-20
---

# M8: Agent Integration & Playbooks

## Checklist
- [ ] MCP server (`rmcp`, stdio and Streamable HTTP), core toolset ([[MCP Server Tool Catalog]]) → accept ADR-0008
- [ ] Async jobs, concise/detailed responses, pagination, actionable errors
- [ ] Playbook language decision (Rhai vs. Lua vs. declarative YAML) and runtime with limits ([[B - Playbooks]])
- [ ] `mtg_playbook_validate`, plus example playbooks for the four test decks
- [ ] Policy composition ([[Policy Composition]])
- [ ] Agent docs: AGENT_GUIDE, llms.txt, Claude Code skill, example specs ([[Agent-Facing Docs Plan]])
- [ ] Agent eval suite: about 15 tasks, measure and iterate

## Exit criteria
Claude completes US-1 end to end via MCP with no human help: it writes playbooks, runs the experiment, and interprets the CIs correctly.
