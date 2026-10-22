---
type: design
status: draft
tags: [policies, llm, agents]
updated: 2026-10-22
---

# C: LLM Live Decisions

## Concept
An LLM answers each decision for a seat. It's an *external* policy driven through the [[MCP Server Tool Catalog]] live toolset (`kiln_live_start` / `kiln_live_decide` / `kiln_live_view`), or a built-in adapter that calls an LLM API directly.

## Role: spot checks and validation, not bulk runs
- **Cost and speed:** prior art (MageBench on XMage) reports ≈ **$1+ per game** and slow play. 5,000 games isn't viable. 10–50 games is.
- **Quality:** commenters judged LLM play weak and inconsistent in competitive formats: hallucinated card text, missed mulligans, poor strategic heuristics ([[LLMs Playing MTG]]). It's still useful for:
  - sanity-checking search or playbook lines ("does this look like how a human would pilot it?")
  - generating *rationale-annotated* replays for humans
  - judge-style "what would you do here?" exploration

## Decision payload design (lessons applied)
- **Always include Oracle text** for every visible card referenced. Agents hallucinate card text otherwise.
- Use **short stable references** (`o17`, `P2`) plus names.
- Pre-filtered legal options only, which we provide by construction.
- Include the **decision context**: why it's being asked, and the source card and CR rule ([[Decision & Action Model]] R3).
- Use **concise vs. detailed views**. The default is a concise board summary with a "detailed" pull on request.
- **Auto-pass trivial decisions** so the LLM isn't asked 300 times per game to pass priority. Configurable: "only ask me at my main phases, on my stack responses, and when targeted".

## Hybrid modes
- **LLM-at-key-moments:** search or a playbook handles routine decisions, and the LLM is consulted only for "major" decisions (mulligans, win attempts, responding to a win attempt). Set up via [[Policy Composition]].
- **LLM writes the playbook** ([[B - Playbooks]]) instead of playing. This is usually the better use of an LLM.

## Reproducibility
LLM runs are marked `reproducible: false`. Their replays are fully reproducible because choices are logged ([[Reproducibility]]).

## Related
[[LLMs Playing MTG]] · [[Policy Composition]]
