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

## As implemented (M10)
- **Live toolset** (`kiln mcp serve --live`): `kiln_live_start` deals game `game` of a kiln-spec/1 pod with the caller on `deck`, replaying `moves` first (games are deterministic, so a new session resumes a game); `kiln_live_decide` answers with an index or label and a rationale, or `auto` to let the seat's spec policy (search, a playbook) take `count` decisions; `kiln_live_view` adds `detailed` and `why_not`. `ask: key` (default) auto-answers trivial decisions with greedy.
- **Payload** (`kiln_sim::live::view_payload`, shared with the adapter): the decision (kind, CR rule, options by index and label), each player's public state and battlefield, the viewer's hand, the stack, events since the last payload (at most 40; cards moving between hidden zones are "a card"), and Oracle text for the cards the decision and hand name (`detailed`: everything visible). A test plays whole games checking no opponent's hidden card is named.
- **Adapter**: `{ kind: llm, model (claude-opus-5-5), ask: key|all, max_calls (200), effort, fallback (greedy) }`. One Messages API request per key decision with structured outputs (`{option, reason}`) and server-side refusal fallbacks; refusals, HTTP errors, unparseable or out-of-range answers fall back and are counted (`llm:answered`, `llm:failed`). LLM-at-key-moments is just `ask: key` with a strong fallback (`fallback: { kind: search }`).
- **Rationales** go into replay `annotations` (`Policy::rationale`): the model's reason, the live caller's, or the options search weighed.

## Related
[[LLMs Playing MTG]] · [[Policy Composition]]
