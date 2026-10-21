---
type: vision
status: accepted
tags: [vision]
updated: 2026-10-20
---

# Vision & Goals

## Vision
A **rules-exact, Commander-first Magic: The Gathering engine** that anyone, and especially **any AI agent**, can use to run large, reproducible simulation experiments and get statistically honest answers about decks, cards and matchups.

The rules engine is the foundation. The **simulation lab is the product**.

## Primary goal (the headline use case)
> "I want to evaluate deck X's performance in a cEDH pod against decks Y, Z and C. Run several thousand simulated games, then report the statistics."

Success means a user (or their AI agent) can:
1. hand over four decklists in common formats ([[Deck Import]])
2. choose how each seat is piloted ([[Policy Composition]])
3. choose which metrics to collect ([[Metrics Catalog]])
4. launch thousands of seeded games that run in parallel ([[Batch Runner & Parallelism]])
5. get back a compact, agent-readable summary plus rich tables and replayable logs ([[Output Formats & Report Schema]])
6. trust the result, because every card is verified to work ([[Validity & Caveats]]), rules are exact, and confidence intervals are reported ([[Statistics for Multiplayer Simulation]])

## Secondary goals
| Goal | What it means | Primary notes |
|---|---|---|
| **Rules judge / Q&A** | Set up a board state, take actions, ask "what happens?" or "why can't I do this?", and get answers that cite CR rule numbers. | [[Judge Tool UX]], [[CLI & REPL]] |
| **Embeddable library** | Other people build clients, bots, research tools and websites on a clean, documented Rust API with Python and WASM bindings. | [[Library API]], [[Python Package]], [[WASM Build]] |
| **Full card pool** | Eventually every card legal in some format, starting with about 32k Commander-legal cards. | [[Card Data Sources]], [[M11 Full Card Pool]] |
| **All major formats** | Commander first, then 1v1 constructed, Two-Headed Giant/team, and Limited. | [[Formats & Game Modes]] |

## Fidelity standard (non-negotiable)
> "You literally shouldn't be able to do things you can't do, but you should be able to customize things when they're within the rules." (the user)

- **Illegal actions are unrepresentable.** The engine only ever offers legal options ([[Decision & Action Model]]).
- **Every choice the CR leaves to a player is a first-class decision.** Examples: ordering simultaneous triggers (CR 603.3b), choosing which replacement effect applies first (CR 616.1), mana payment, targets, modes, X, damage division (CR 510.1c), "may" abilities, shortcut lengths (CR 732.2a).
- **Convenience never hides a choice.** Auto-ordering and auto-payment exist only as explicit, opt-in policy behaviour.

## Measures of success
- A 5,000-game 4-player cEDH experiment completes in a time a user will wait for (see [[Non-Functional Requirements]] for targets per policy kind).
- 100% of cards in an experiment are in "verified" coverage status. Otherwise the run refuses, or is flagged ([[Validity & Caveats]]).
- Every implemented rule has at least one CR-cited scenario test ([[CR-Cited Scenario Tests]]).
- Same spec plus same seed gives a bit-identical report ([[Reproducibility]]).
- An AI agent can run the headline experiment end to end using only the [[Agent-Facing Docs Plan]] docs.

## Related
[[Scope & Non-Goals]] · [[Design Principles]] · [[User Stories]]
