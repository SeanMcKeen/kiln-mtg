---
type: milestone
status: review
milestone: M9
tags: [milestone, ai]
updated: 2026-10-27
---

# M9: Search AI

## Checklist
- [x] Determinization service in the engine ([[Policy Interface]])
- [x] SO-ISMCTS with iteration and time budgets; opponent-model backups instead of max^n (Q31, [[ADR-0014 Search AI Design]])
- [x] Playouts (greedy) and progressive widening
- [x] Evaluation function for truncated playouts
- [x] Playbook-guided search (the playbook gives the prior and answers what search skips; playouts stay greedy for speed)
- [x] Puzzle suite and policy ladder ([[Measuring Play Quality]])

## Exit criteria
Meets the v1 bar in [[Measuring Play Quality]]. The US-1 run with guided search completes 5,000 games in under 1 hour on 8 cores.

## Starting point (2026-10-27)
- M8 delivered Rhai playbooks, policy composition, the MCP server and the in-chat agent eval. Policies: random, greedy (now naming cards sensibly), passive, playbook, compose. The `search` spec kind is parsed and played as greedy.
- `kiln-search` is an empty stub. `PlayerView::game_copy` exists for playbooks; there is no determinization.
- Measured: a mid-game 4-player clone takes ~0.45 µs (synthetic board); a whole synthetic 4-player game ~3 ms; a real-deck greedy pod ~2,700 decisions per game at ~70 games/s per thread. Rollouts, not clones, set the cost.

## Plan
### Design decisions
| # | Decision | Why |
|---|---|---|
| S1 | **Determinization in the engine**: `PlayerView::determinize(seed) -> Game`. Each other player's hand and library are pooled and redealt (sizes kept); the viewer's library is reshuffled except a top card it may see; the game RNG is reseeded. Face-down exile stays. | Sampling on the engine side means hidden information never reaches the policy ([[Policy Interface]]). Pooling hand and library is exactly what the viewer can't tell apart. |
| S2 | **Opponents' hidden cards come from their real decklists** (open decklists). The report warns when search runs with `open_decklists: false`. | cEDH lists are public; sampling unknown cards from a meta model is a later refinement. |
| S3 | **SO-ISMCTS over the searching player's own decisions.** Nodes are its decision sequences, children keyed by option label (stable across determinizations), with availability counts (subset-armed bandits). Opponents act inside each simulation through an opponent model (greedy, or their configured policy's fallback). | With tens of iterations per decision, opponent nodes in a 4-player tree would get no visits. This is the "opponent model" backup: each player's value is its own expected outcome. It resolves Q31 for v1; max^n trees can come back when budgets allow. |
| S4 | **PUCT selection in integer fixed point** (values in thousandths, `isqrt`), priors from the guide policy: its pick gets half the mass. **Progressive widening**: a node considers `1 + isqrt(visits)` children in prior order (guide pick, then pass, then the rest). | ADR-0005: no floats in anything that drives game logic; integer maths is identical on every platform. Widening keeps large option lists (mana abilities, many targets) cheap. |
| S5 | **Rollouts** with the guide policy for every seat, cut off after `rollout_depth` decisions (default 300), then the **evaluation function**: per living player 4×life share + cards in hand + 2×lands + creature power share + mana rocks, poison and commander damage as penalties; value = own score ÷ total. Terminal states score 1000 / 0 / shared draw. | Rollout quality matters more than iteration count (Cowling et al.); truncation bounds the cost of 40-turn greedy games. |
| S6 | **Search only where it matters**: priority with two or more candidates, targets, attackers, blockers, card choices (search, take, discard), mulligans and modes. Payments, ordering and the rest go to the guide policy. | Decisions are factored; most are trivial. |
| S7 | **Playbook-guided search**: with `playbook`, the playbook (→ greedy) is the guide: its answer is the prior, and it pilots the searching seat in rollouts. | [[Policy Composition]] "best of both". |
| S8 | **Budgets**: `budget: { iterations, rollout_depth, ms }`. Iterations (default 64) are reproducible; `ms` is a cap that makes results machine-dependent, so the report warns (like `max_wall_ms`). Each decision's search seed comes from the policy seed and a decision counter. | Reproducibility (ADR-0005). |
| S9 | **Puzzle suite**: `tests/puzzles/*.yaml`, a kiln-scenario/1 board plus `puzzle: { solver, kind: win_now / stop_the_win, within_turns, opponents }`. `kiln dev puzzles --policy search` reports the solve rate. | [[Measuring Play Quality]] method 2. |
| S10 | **Ladder**: mirror pods with two search and two greedy seats (same deck, alternating), win share of the search seats; budget monotonicity from paired runs at increasing budgets. | The v1 bar: ≥ 70% share vs greedy, monotone in budget. |
| S11 | **Power**: development runs use 2 threads; the 8-core throughput is extrapolated from measured per-game time. | The user's laptop (memory: power usage). |

### Phases
| Phase | Scope | Check |
|---|---|---|
| 9.1 | Determinization service (`kiln-engine/src/determinize.rs`), invariants and tests | The determinized game's view equals the original's; hidden multisets kept; the redeal varies with the seed |
| 9.2 | `kiln-search`: tree, PUCT, widening, rollouts, evaluation, budgets; `SearchPolicy` | Unit tests: takes lethal, holds a counter, deterministic |
| 9.3 | `kind: search` in specs (typed budget), report warnings, protocol schemas, docs | A US-1 pod with search runs and replays |
| 9.4 | Puzzle suite + `kiln dev puzzles` | Solve rates vs greedy |
| 9.5 | Ladder, budget monotonicity, throughput, tuning; ADR-0014 (search design); docs and merge | The v1 bar, throughput extrapolation |

## Progress
| Phase | Status | Notes |
|---|---|---|
| 9.1 | ✅ 2026-10-28 | `PlayerView::determinize` (`kiln-engine/src/determinize.rs`): hidden cards redealt by permuting definitions, RNG reseeded; tests check the viewer's snapshot is unchanged and the hidden multisets are kept. |
| 9.2 | ✅ 2026-10-28 | `kiln-search`: own-decision SO-ISMCTS, integer PUCT, widening, greedy playouts, `evaluate_after` (discounted wins, late losses, heuristic 50–800), once-per-turn activations and 40 searches a turn as loop guards, early stop at half budget when settled, `last_search()`. |
| 9.3 | ✅ 2026-10-28 | `{ kind: search, budget: { iterations, rollout_depth, ms }, guide }` (typed; replaces the M7 placeholder), validation, report recipe and warnings (open decklists, ms budgets), checkpoints every few games for search runs. |
| 9.4 | ✅ 2026-10-28 | 14 puzzles in `tests/puzzles/` (`kiln-puzzle/1`), `kiln dev puzzles`, `kiln_testkit::puzzle`; a real-data test holds the v1 bar. |
| 9.5 | ✅ 2026-10-28 | Ladder, budget and throughput runs below; [[ADR-0014 Search AI Design]] accepted. |

## Results (2026-10-28, 2 threads on the development laptop)
| Check | Result | Bar |
|---|---|---|
| Puzzles, default budget (32 iterations) | win now 8/9 (89%), stop the win 5/5 | ≥ 80%, ≥ 60% ✅ |
| Puzzles, greedy | 2/9, 4/5 | — |
| Puzzles by budget (8 / 32 / 128) | win now 5 / 8 / 8 of 9; stop 5/5 throughout | monotone ✅ |
| Najeela mirror, 2 search vs 2 greedy seats, 100 games | search seats won 82 (~[73, 88]%) | ≥ 70% ✅ |
| Najeela mirror, 2 × 32 vs 2 × 8 iterations, 100 games | 32-iteration seats won 63 (~[53, 72]%) | > 50% (monotone) ✅ |
| Najeela on search vs 3 greedy decks, 100 games | 71.0% [61.5, 79.0] (greedy pilot: 44.0%) | — |
| Search game replays | `diverged: false` | ADR-0005 ✅ |
| Throughput: one search seat, greedy guide | ~3.0 s per game per thread | — |
| Throughput: Kinnan, playbook-guided (US-1 spec) | ~5.4 s per game per thread → 5,000 games on 8 cores ≈ 56 min (extrapolated) | < 1 h, borderline ✅ |

Not done: the full 5,000-game guided US-1 run (about 7.5 hours on 2 threads; `examples/specs/m9-us1-guided.yaml`), and the seat-1 check (Q39) with search pilots. The deep Walking Ballista + Heliod loop stays unsolved: playouts are greedy, so lines longer than the tree's reach need a playbook guide.

## Related
[[D - Search AI]] · [[Measuring Play Quality]] · [[MCTS & ISMCTS for MTG]] · [[Policy Interface]]
