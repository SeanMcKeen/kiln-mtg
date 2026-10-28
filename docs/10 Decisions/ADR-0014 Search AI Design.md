---
type: adr
status: accepted
decided: 2026-10-28
supersedes:
tags: [adr, ai, search]
updated: 2026-10-28
---

# ADR-0014: Search AI Design

## Context
[[ADR-0006 Pluggable Decision-Makers]] puts a search player in scope; [[D - Search AI]] sketched SO-ISMCTS with determinization, greedy or playbook playouts, an evaluation function and max^n backups. M9 had to make it concrete under three constraints: results must replay bit-for-bit ([[ADR-0005 Determinism and Replay]]); policies never read hidden information ([[Policy Interface]]); and the US-1 run (5,000 four-player games) must finish within an hour on 8 cores, which leaves about 6 seconds of search per game per core. Real-card playouts cost about 20 µs per decision (M9 measurements), so a decision gets tens of playouts, not thousands.

## Options
1. **Full multiplayer ISMCTS** with every player's decisions in the tree and max^n vector backups. Principled, but with tens of iterations per decision, opponents' nodes get almost no visits in a four-player tree.
2. **Flat Monte Carlo** (one level: try each option, play out). Cheap, but blind to sequences of the searcher's own decisions (cast → target → respond), which is where cEDH lines live.
3. **SO-ISMCTS over the searcher's own decisions, opponents as an environment** played by a model (greedy), with determinized samples per iteration.

## Decision
Option 3, as built in `kiln-search` (M9):
- **Determinization in the engine** (`PlayerView::determinize`): other players' hands and libraries are pooled and redealt, the viewer's library reshuffled (except a visible top card), the random stream reseeded. Opponents' hidden cards come from their real decks (open decklists; the report warns when `open_decklists` is false).
- **Tree of the searcher's decisions**, children keyed by option label (stable across samples), availability counts for options a sample doesn't offer, one node expanded per iteration. Search covers priority (two or more non-mana actions), targets, attacks, blocks, card choices, mulligans, modes, names, players and "may" choices; payments and ordering go to the guide.
- **PUCT in integer arithmetic** (thousandths, `isqrt`): exploration 0.7, the guide's pick 40% of the prior. Priority decisions widen progressively (`1 + isqrt(visits)`, at least 3 options); other decisions consider up to 8 options at once. The final choice is the best mean value among options tried at least twice and a quarter as often as the most tried.
- **Playouts** with greedy for every seat, cut after `rollout_depth` real choices (default 100). **Values**: a win is 1000 less 25 per turn taken (at least 850); a loss 5 per turn survived (at most 40); unfinished positions 50–800 by the searcher's share of life, cards, lands, permanents and creature power.
- **Guards**: an ability is activated at most once a turn (as greedy does), and after 40 searched decisions in a turn the guide takes over, so loops and long combo turns stay bounded.
- **Budget**: `budget: { iterations (32), rollout_depth (100), ms }`. Iterations are reproducible; `ms` makes results machine-dependent and the report warns.
- **Guided search**: `guide` (greedy by default, or a playbook with a fallback) answers what search skips and gives the prior.

This resolves Q31 for v1: backups are each player's own expected outcome under an opponent model; max^n trees can return when budgets allow.

## Consequences
- Search beats greedy on the puzzle suite (M9: 8/9 win-now, 5/5 stop-the-win at the default budget, against greedy's 2/9 and 4/5), wins 82% of a mirror against greedy seats, and improves with budget (63% for 32 against 8 iterations); see [[M9 Search AI]].
- Playouts are greedy for every seat, the searcher's too: a playbook call per playout decision made guided search five times slower. The playbook guides through the prior and the decisions search hands it.
- At half budget, search stops when its options are within 15 thousandths of each other or one leads by 200, which keeps guided US-1 near 5.4 s per game per core.
- Its opponent model is greedy: it doesn't anticipate interaction that greedy wouldn't make, and deep combo loops (Walking Ballista with Heliod) stay beyond its horizon: a playbook guide can steer the first steps through the prior, but playouts don't follow it.
- Every value and choice is an integer, so a search with an iteration budget replays exactly.

## Related
[[D - Search AI]] · [[M9 Search AI]] · [[Measuring Play Quality]] · [[MCTS & ISMCTS for MTG]]
