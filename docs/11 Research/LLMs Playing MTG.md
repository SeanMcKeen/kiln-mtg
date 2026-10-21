---
type: research
status: draft
tags: [research, llm, ai]
updated: 2026-10-20
---

# LLMs Playing MTG

## Projects
| Project | Setup | Findings |
|---|---|---|
| **MageBench** (mage-bench.com) | LLMs play via MCP tools wired to **XMage**. A `pass_priority` → read choices → `choose_action` loop. Short IDs (e.g. "p3"), pre-filtered castable cards, Oracle text added for visible cards. | ≈ **$1+ per game**. Hallucinated card text and costs, harness bugs around triggers. Commenters judged Legacy play too weak to trust (missed mulligans, no beatdown awareness). Little negotiation in Commander. Leaderboard snapshot 2026-05-28 with few games for frontier models. |
| **MTG Bench** | The LLM tracks state itself, with only library operations via MCP | Frequent state errors (e.g. forgetting exiled cards). **The engine must own state.** |
| **UrzaGPT** (arXiv 2508.08382) | LoRA-tuned LLM for **draft picks** | Draft only, not gameplay. Relevant to future Limited support. |

## Takeaways
- LLMs are a **poor bulk pilot** (cost, speed, quality) but useful for **writing playbooks** and **spot-check review**. That matches [[C - LLM Live Decisions]] and [[B - Playbooks]].
- The payload must include Oracle text, pre-filtered options, decision context, and short stable references.
- Commenters suggested Monte Carlo or classical AI for deck evaluation, which supports [[D - Search AI]] as the default competent pilot.

## Sources
- HN thread: https://hn.svelte.dev/item/47049227
- MageBench listing: https://benchmarklist.com/benchmarks/mage_bench_season1/
- MTG Bench coverage: https://feedbagel.com/post/mtg-bench-testing-how-well-llms-can-play-magic
- UrzaGPT: https://arxiv.org/html/2508.08382v1
