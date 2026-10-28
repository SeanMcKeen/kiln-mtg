---
type: plan
status: active
tags: [agents, evals, mcp]
updated: 2026-10-27
---

# Agent eval suite

Fifteen realistic tasks (`tasks.yaml`) for an agent connected to `kiln mcp serve`. They measure whether an agent with no prior knowledge can use Kiln well ([[Agent-Facing Docs Plan]], FR-I5) and feed back into tool descriptions and the [[AGENT_GUIDE]]. They cover:
- the headline deck evaluation (US-1, plus a playbook variant that is the M8 exit criterion),
- goldfish speed (US-2) and opening hands (US-4),
- A/B swaps, the coverage gate, misspellings and error recovery,
- inspection, statistics traps, per-card caveats, rules questions, cancel and resume, and comparison.

## Running
1. Build `kiln` and the card DB. Start each task in a fresh session with a fresh `--workdir`, connected only to the `kiln` server, and give the agent the task's `prompt` verbatim.
2. Record per task: success (every check met), the number of tool calls, tokens, errors from tools, and anything confusing in the tool descriptions.
3. Grade from the transcript and the run directories under the workdir. `tools/mcp-smoke/smoke.py` checks the server itself.

**Running the suite with a live model spends API usage**, so it waits for the user's go-ahead (M8 plan P8). The first run was done in-chat instead: the Claude Code session acted as the agent and drove `kiln mcp serve` through `tools/mcp-smoke/session.py`, which runs a JSON list of tool calls in one server session (`{"wait": run_id}` polls until a run settles, `{"sleep": s}` pauses, and an `@path` argument is read from a file).

## Results
| Date | Model | Passed | Notes |
|---|---|---|---|
| 2026-10-27 | Claude Opus 5.5, in-chat (not blind) | 15/15 | The evaluator built Kiln and wrote the guide, so this measures the tools and the docs, not a cold start. Tool calls exclude status polls. |

### 2026-10-27 run, per task
| Task | Pass | Tool calls | Notes |
|---|---|---|---|
| us1-pod | yes | 17 (9 polls) | All checks met. Findings: concise `next` hints use CLI syntax over MCP; concise by-seat rates have no intervals (seat 4 33.2% on ~500 games invites over-reading). |
| us1-playbooks | yes | ~22 (mostly polls) | Wrote a fresh Kinnan playbook (mulligans, land drops, Kinnan and cheap rocks first, Brain Freeze never at itself); compiled first time, no runtime errors. Only Kinnan's policy changed (same seed and seating): 28.5% → 28.2% (−0.3 pp, p = 0.81): no claim of improvement. Findings: kiln_sim_compare uses an unpaired test and an "independent runs" note even when both runs share seeds and seating (a paired test fits); the different-policy-kinds warning fires on deliberate single-pilot comparisons. |
| us2-goldfish | yes | 4 (+1 detailed) | Najeela goldfish: median win on its own turn 10 [10, 11] (p10 9, p90 13); 32.9% of games capped at max_turns 60. Findings: the concise report has no median interval (detailed has it); the goldfish deck kept playing after decking itself (the passive opponents play to the cap, so self-losses count as capped); greedy casts Demonic Consultation naming a card not in its library and decks itself; seat-bias notable is meaningless in goldfish. |
| us4-hands | yes | 3 | Kinnan keeps 7 in 60.3% [58.1, 62.4]; 23.7% keep at 6, 16.0% at 5 or fewer; 20.5% [18.7, 22.3] of kept hands have fast mana. Findings: `kept_at` entries aren't labelled with the hand size; `mode` prints as `openinghands`; `games: 0` in an opening-hands report. |
| ab-swap | yes | 3 | Sol Ring for Abrupt Decay: +1.45 pp [−1.2, +4.3] over 1,995 paired games: the interval includes zero, so can't tell. |
| coverage-gate | yes | 2 | Import: legal, 18 unsupported cards listed, experiment_ready false; the run refused with the list. Offered kinnan-verified.txt. Findings: the refusal names no fix; `unsupported_reasons` counts 26 for 18 cards; basic Forest/Island are parsed-not-verified. |
| misspelled-card | yes | 2 | "Thassas Oracle" resolved to Thassa's Oracle (verified); "Demonic Consultaton" gave did-you-mean, the follow-up lookup shows Demonic Consultation verified. |
| bad-spec-recovery | yes | 3 | The error named `gmaes` and suggested `games`; the fixed spec ran 100 games. |
| inspect-fast-win | yes | 2 | Fastest win in the last pod run (game 658) is degenerate: three opponents cast Demonic Consultation blindly and decked; Kinnan won by decking on its turn 3. Findings: the next hint uses CLI syntax; events don't show the chosen name. |
| seat-bias | yes | 1 | us1-pod: seat 1 23.4% [21.6, 25.3] vs seat 4 26.5% [24.7, 28.5], no seat-1 edge; noted real cEDH has one and greedy pilots don't show it (Q39). |
| per-card | yes | 2 | Paginated per-card report (10 pages) with the correlational note; recommended A/B for causal answers. Findings: no sort (by IWD); `first_cast_turn` is the game turn while win turns are own turns. |
| rules-oracle | yes | 5 | Wrote a kiln-scenario/1 board; probing unknown labels listed the options; the engine reports win P1 on turn 3. Findings: `result` is a raw enum with a 0-based player; events don't show the named card or the exile. |
| cancel-resume | yes | 5 + polls | Cancelled at 667/20,000; re-running the spec resumed from 667 and completed (121 s, 2 threads). Findings: the cancel call answers `running`; a cancelled status says "poll again" instead of how to resume; no suggested poll interval (1 s polling cost 120 calls). |
| compare-runs | yes | 1 | Reported differences with p-values; the note warns independent runs differ for any reason. |
| small-sample | yes | 3 | 52 games (rounded up for rotation): ±13.2 pp intervals, Najeela and Kinnan overlap; declined to name a best deck and recommended ~5,000 games. Finding: concise notable lines (seat 1 vs 4 −13.7 pp, "highest win rate") show no uncertainty on tiny samples. |

### What changed because of it
- **The greedy pilot named cards blindly.** For Demonic Consultation it took the first name in the pool, usually one missing from its library, and decked itself. It now names a card still in its library (`PlayerView::own_library_contents`: contents, never order, CR 401.2) unless an empty library wins (an Oracle trigger on the stack, a Lab Maniac in play). Pod results moved: on `us1-pod`'s seeds Kinnan fell from 28.5% to 23.0% (it had been winning when opponents decked), Thrasios rose from 18.9% to 22.6% and Najeela from 40.3% to 44.0%. Numbers recorded before this change (M6–M8 notes) are from the older pilot.
- **Goldfish games end when the measured deck is out** (record outcome `lost`): the passive opponents can't finish the game, so self-losses used to sit at the turn cap as capped games. Najeela's capped share fell from 32.9% to 19.1%; the rest is the 60-turn cap (15 of its own turns) being too short to kill three 40-life opponents with combat.
- **MCP hints are tool calls**: the concise report's `next` reads `kiln_sim_report run_id=… format=detailed` and `kiln_game_inspect run_id=… game_index=…`, and `kiln_sim_status` suggests a poll interval, answers `cancelling` to a cancel, and tells a cancelled run how to resume.
- **Notable lines carry intervals** and say when the seats or the leading decks overlap; goldfish skips the seat line, and the median win turn shows its interval.
- **`kiln_sim_compare` pairs runs** that share seeds and seat orders and reports `paired_ci95` per deck (Kinnan's playbook: −0.3 pp, paired [−2.7, +1.8]).
- Smaller fixes: per-card `page` is 1-based; `mode` prints as `opening_hands`; scenario results read `win P1 on turn 3`; the unsupported-card refusal says how to fix it; `unsupported_reasons` counts each card once per reason.

### Follow-up fixes (same day)
- Per-card stats sort with `sort` (`seen`, `iwd`, `gih_wr`, `oh_wr`, `cast_wr`, `dead`), and `first_cast_turn` is the deck's own turn.
- **Opening hands are dealt in a 4-player pod**, so the first mulligan is free (CR 103.5c); it used to be a 2-player game, which understated Commander keeps. `hand_sizes` labels `kept_at` (7, 7, 6, … in multiplayer), and the concise report counts hands as games.
- Events show the chosen name (`P2 chooses Diabolic Intent`) and cards exiled from a library as one line.
- Basic lands are verified (`tests/scenarios-real/lands-basics.yaml`).
- Q41 resolved: turn limits stay in game turns, the goldfish example uses the default 100 (25 own turns; capped 32.9% → 1.4%), and the capped warning gives turns per player. The fastest-win hint ranks by the winner's own turn and puts wins by opponents decking last.

### Still open
- Concise reports hold no per-seat intervals in `win_rates` (the notable line has the seat-1 vs last-seat intervals; `detailed` has all of them). Kept: the concise report stays near 2k tokens.
- Inspecting a run recorded before a policy change replays with the new policy and reports `diverged: true`; re-run the spec to inspect it.

## Related
[[M8 Agent Integration & Playbooks]] · [[MCP Server Tool Catalog]]
