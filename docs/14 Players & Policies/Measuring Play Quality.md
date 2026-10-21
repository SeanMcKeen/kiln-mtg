---
type: design
status: draft
tags: [policies, evaluation, validity]
updated: 2026-10-20
---

# Measuring Play Quality

Without measurement, "the search AI plays decently" is just a claim ([[Design Principles]] §10).

## Methods
1. **Policy ladders:** round-robin the same decks under different policies (random < greedy < playbook < search < guided search). Expect monotone improvement, and track Elo or TrueSkill per policy.
2. **Puzzle suite ("find the win"):** curated positions with a known winning line, such as:
   - Thoracle + Consultation available with 1 counter held by an opponent
   - Breach + LED in hand
   - "must counter this tutor"

   Measure the solve rate and time. This doubles as a regression test.
3. **Goldfish speed vs. known benchmarks:** compare a deck's simulated goldfish kill turn to community-reported speeds. A rough sanity check.
4. **Ground-truth comparison:** simulated meta win rates vs. real tournament conversion rates ([[cEDH Ground-Truth Data]]). Weak evidence, but useful for directional validation.
5. **LLM or human review:** sample replays and have an LLM or an expert annotate blunders ([[C - LLM Live Decisions]]). MageBench's "blunder index" was found unreliable, so use structured rubrics.
6. **Self-consistency:** more budget should never hurt in expectation. Test with paired seeds.

## "Good enough" bar for v1 (M9)
- Solves ≥ 80% of the puzzle suite's "win now" puzzles and ≥ 60% of "stop the win" puzzles.
- Beats `greedy` with ≥ 70% head-to-head win share in mirror pods.
- Monotone in budget.

## Related
[[D - Search AI]] · [[Validity & Caveats]] · [[M9 Search AI]]
