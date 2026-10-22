---
type: design
status: draft
tags: [policies, search, mcts, ai]
updated: 2026-10-22
---

# D: Generic Search AI

## Concept
A deck-agnostic look-ahead player. At non-trivial decisions it samples possible hidden states, simulates continuations, and picks the option with the best expected outcome. It lives in the `kiln-search` crate. It's in scope per [[ADR-0006 Pluggable Decision-Makers]], which reversed the original "AI out of scope" decision.

## Algorithm family
- **Determinized MCTS / Perfect-Information Monte Carlo (PIMC):** sample a determinization (opponents' hands and library order consistent with the view), run MCTS on it, and aggregate across samples. Cowling, Ward & Powley (2012) applied **ensemble determinization** MCTS to MTG and beat an expert rule-based player.
- **Information Set MCTS (ISMCTS)** (Cowling, Powley & Whitehouse 2012): a single tree over information sets, which addresses PIMC's **strategy fusion** and non-locality problems. Whitehouse's thesis reports ISMCTS outperforming PIMC.
- **Multiplayer:** use per-player value vectors (max^n-style backups) rather than two-player minimax.

**Starting point:** SO-ISMCTS with:
- determinization via the engine's sampling service ([[Policy Interface]])
- **playouts with a fast default policy** (greedy, or playbook if available) rather than random, which matters hugely in MTG
- **progressive widening** and action pruning for large option sets (also a lesson from Cowling et al.)
- a **budget** in iterations (reproducible) or milliseconds
- an evaluation function for truncated playouts: life totals, card advantage, board, "combo pieces assembled", commander damage

## Playbook-guided search
If a deck has a playbook, use it as:
1. a **prior** over actions (PUCT-style)
2. the **rollout policy**

This makes search both smarter and cheaper. It's the "best of both" composition ([[Policy Composition]]).

## Cost model
Clone cost and playout speed set quality per millisecond ([[Performance & Cheap Cloning]]). See the budget table in [[Non-Functional Requirements]]: ~60 non-trivial decisions per cEDH game × 10 ms gives ~6 minutes for 5,000 games on 8 cores.

## Known weaknesses
- **Long combo chains:** the horizon is too short to "see" a 15-step Breach line. Mitigate with playbook macros, loop shortcuts ([[Loops & Shortcuts]]), and combo-aware evaluation.
- **Bluffing and deep hidden-information reasoning:** determinization is weak here.
- **Kingmaking dynamics** in 4-player games.

## Related
[[MCTS & ISMCTS for MTG]] · [[Measuring Play Quality]] · [[M9 Search AI]]
