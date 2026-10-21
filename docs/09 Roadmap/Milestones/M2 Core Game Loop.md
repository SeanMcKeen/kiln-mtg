---
type: milestone
status: planned
milestone: M2
tags: [milestone]
updated: 2026-10-20
---

# M2: Core Game Loop

**Goal:** an N-player engine that plays complete games with lands, mana, vanilla and French-vanilla creatures, simple spells (hand-coded IR), combat, SBAs, deterministic replays, and a random policy.

## Checklist
- [ ] `GameState`, object arena, zones ([[Game State Model]], [[Zones & Objects]])
- [ ] Decision model v1: `PendingDecision`/`Choice`, continuations ([[Decision & Action Model]]) → accept ADR-0004
- [ ] Turn structure and turn-based actions ([[Turn Structure]])
- [ ] Priority and the stack, resolution ([[Priority & the Stack]], [[Resolution]])
- [ ] Mana pools, mana abilities, cost payment v1 with plan enumeration ([[Costs & Mana]])
- [ ] Casting procedure skeleton per CR 601.2 ([[Casting & Activating]])
- [ ] Combat with factored declarations ([[Combat]])
- [ ] Core SBAs (704.5a–j) ([[State-Based Actions]])
- [ ] Mulligans (London, multiplayer free mulligan)
- [ ] Seeded RNG, replay log, determinism tests → accept ADR-0005 ([[Determinism & Replay]])
- [ ] PlayerView v1 with knowledge tracking basics ([[Hidden Information & Player Views]])
- [ ] Event system v1 ([[Event System]])
- [ ] `mtg-policy`: the Policy trait, random, passive, scripted
- [ ] Sim skeleton: run N random games in parallel. Invariant checks ([[Invariants & Property Tests]])
- [ ] Benchmarks: clone cost, games/sec → decide the persistent vs. arena state strategy ([[Performance & Cheap Cloning]])
- [ ] Scenario file loader v1 ([[Scenario File Format]])

## Exit criteria
1M random 4-player games with vanilla decks: no panics, no invariant violations, 100% replay determinism. Benchmarks recorded.

## Implementation plan (draft for review, written at the end of M1)
> [!warning] Depends on ADR-0012
> This plan assumes the M1 recommendation, **E3 (borrow selectively)**: a clean-room engine core. If the user picks E1 (depend) or E2 (fork) instead, this plan is replaced.

Lessons from [[phase.rs Evaluation]] are built in as hard requirements:
1. **Offered ⇒ applicable, tested.** A CI property test applies *every* offered option to a clone (invariant 3 in [[Invariants & Property Tests]]). phase.rs offered actions its engine then rejected in about 1 of every 1,300 random actions.
2. **Enumeration cost is a design input.** Priority options are maintained incrementally (indices by zone and type), not recomputed from scratch. Target: NFR-P3, ≤ 20 µs. phase.rs measured about 4 ms per `legal_actions` call.
3. **Policies receive only a `PlayerView`.** Perfect information isn't reachable through the policy API, whatever a config says.
4. **No silent drops anywhere.** Unknown names, unsupported cards and dropped actions are errors or explicit report entries.

### Phases
| # | Work | CR | Done when |
|---|---|---|---|
| 2.1 | `GameState`: players, zones, generational object arena (a zone change makes a new object), `TurnState`, ChaCha8 RNG with hash-split per-game seeds, structural state hash | 400.7, 103 | Unit tests; hash stable across clones |
| 2.2 | Decision model v1: `PendingDecision`/`Choice`, monotonic `DecisionId`, hand-written continuation enums (Q11: start with enums, revisit at M4) | 117 | Debug-build invariant harness runs after every `apply` |
| 2.3 | Turn structure, turn-based actions, priority (APNAP), stack, resolution, game end, leaving the game | 500–514, 117, 405, 608, 104, 800.4a | Scripted scenarios pass |
| 2.4 | SBAs 704.5a–j, including the legend rule as a decision | 704 | Each SBA has a CR-cited scenario |
| 2.5 | Mana: pools, intrinsic basic-land mana abilities, payment-plan enumeration with outcome de-duplication, manual mode (Q13) | 106, 305.6, 601.2g–h, 605 | Every plan is legal (property test) |
| 2.6 | Casting skeleton per 601.2a–i for vanilla permanents and simple spells. Land play as a special action | 601, 305.1, 116.2a | Casting scenarios pass |
| 2.7 | Combat, factored per creature: attack targets (player, planeswalker, battle), blocks, first/double strike, damage division per 510.1c–e. French-vanilla keywords (flying, reach, vigilance, haste, trample, deathtouch, lifelink, menace, defender) | 506–511, 702 | Combat scenarios pass |
| 2.8 | Mulligans: London, plus the multiplayer free first mulligan | 103.5, 103.5c | Scenario tests |
| 2.9 | `PlayerView` v1 plus redaction auditor. `mtg-policy`: `Policy` trait, random, passive, scripted. Trivial-decision fast path | 400.2 | Auditor finds no leaks across 10k games |
| 2.10 | Replay log `(decision_id, option_index)` and a replay runner that reports the first divergence | — | Replays are 100% identical over the fuzz corpus → accept ADR-0004 and ADR-0005 |
| 2.11 | Sim skeleton: rayon batch runner, synthetic vanilla decks, invariant checks, 1M-game fuzz run | — | Exit criterion |
| 2.12 | Benchmarks (criterion): clone, games/s, priority enumeration. Start with plain `Vec` arenas plus `Arc`-shared definitions (strategy A/C); try `im` only if clone > 10 µs (Q10) | — | Numbers recorded in [[Non-Functional Requirements]] |
| 2.13 | Scenario file loader v1 (YAML, [[Scenario File Format]]) for the hand-coded M2 test cards | — | Scenarios run from data files |

**Cards in M2:** a deliberately tiny, hand-written test-card set: basic lands, vanilla and French-vanilla creatures, and a few burn and draw spells. It lives in a temporary M2 IR so it can't pre-empt ADR-0007. It's replaced by the real IR in M3.

**Order:** 2.1 → 2.2 → 2.3/2.4 → 2.5/2.6 → 2.7 → 2.8 → 2.9 → 2.10 → 2.11/2.12 → 2.13. Benchmarks (2.12) get a first pass right after 2.6, so the state strategy is chosen before combat code piles up.
