---
type: vision
status: draft
tags: [vision, user-stories, requirements]
updated: 2026-10-20
---

# User Stories

Each story traces to the notes and milestones that deliver it. The headline story (US-1) is the acceptance test for the whole project.

## US-1: Evaluate a deck in a cEDH pod *(headline)*
> As a cEDH player, I ask my AI assistant: "Evaluate my Kinnan list against Blue Farm, Najeela and Thrasios/Tymna over 5,000 games and tell me how it performs."

**Flow:**
1. The agent calls `mtg_deck_import` four times (Moxfield URL, pasted text, or file). Each call returns a deck ID, legality, and **coverage** (any unverified cards are listed). → [[Deck Import]], [[MCP Server Tool Catalog]]
2. The agent writes an experiment spec: 4 decks, `seating: balanced_rotation`, policies per seat (e.g. `search` with a per-deck playbook), `games: 5000`, `seed`, and metric flags. → [[Experiment Spec]], [[Policy Composition]]
3. `mtg_sim_run` returns a job ID. The agent polls `mtg_sim_status`, which reports progress and an ETA. → [[Batch Runner & Parallelism]]
4. `mtg_sim_report` with `response_format: concise` returns:
   - win rate per deck, with 95% CIs and seat breakdown
   - draw rate
   - mean and median win turn
   - win-type mix
   - mulligan stats
   - the top 10 over- and under-performing cards

   → [[Metrics Catalog]], [[Output Formats & Report Schema]]
5. The agent drills in: "show me a game where Kinnan lost to Najeela on turn 3". It calls `mtg_game_inspect` on a replay. → [[Golden Replays]], [[Reproducibility]]

**Delivered by:** [[M6 Commander & cEDH Core Pool]] (rules and cards), [[M7 Simulation Lab v1]] (harness), [[M8 Agent Integration & Playbooks]] (MCP, playbooks), and [[M9 Search AI]] (competent play).

## US-2: Goldfish speed test
> "How fast does this deck win with no interaction? What's my average turn to Thoracle?"

This is a goldfish experiment: one deck, null opponents, and a win-turn distribution. It's achievable early, at [[M7 Simulation Lab v1]] with playbook or greedy policies.

## US-3: Card swap A/B test
> "Is Mystic Remora better than Rhystic Study in this list?"

The flow is a paired experiment. Two deck variants use the **same seeds**, which gives common random numbers and reduces variance. The report shows the difference in win rate with its CI. → [[Experiment Spec]] (`variants`), [[Statistics for Multiplayer Simulation]]

## US-4: Opening-hand / mulligan analysis
> "What % of my 7-card hands are keepable under my mulligan rules? How often do I have a turn-2 win?"

This uses a cheap "opening hands only" mode with no full game, and playbook mulligan rules. → [[Metrics Catalog]] (mulligan flags), [[B - Playbooks]]

## US-5: Rules judge question
> "If I cast Thassa's Oracle with Notion Thief on the battlefield and my library is empty, do I win?"

The judge sets up the scenario, steps through it, and gets an explanation citing CR rule numbers. → [[Judge Tool UX]], [[CLI & REPL]], [[M10 LLM Bridge & Judge]]

## US-6: Spot-check play quality with an LLM
> "Let Claude pilot seat 1 for 20 games so I can see whether the search AI's lines look sane."

The flow is a live LLM policy on one seat, plus annotated replays. → [[C - LLM Live Decisions]], [[Measuring Play Quality]]

## US-7: Library consumer
> "I'm building a web app / RL agent and want to embed the engine."

→ [[Library API]], [[Python Package]], [[WASM Build]]

## US-8: Validate against reality
> "Do the simulated meta win rates resemble real cEDH tournament results?"

The flow imports top decklists from tournament data, simulates, and compares. → [[cEDH Ground-Truth Data]], [[Validity & Caveats]]

## Related
[[Vision & Goals]] · [[Functional Requirements]]
