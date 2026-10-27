---
type: milestone
status: review
milestone: M2
tags: [milestone]
updated: 2026-10-22
---

# M2: Core Game Loop

**Goal:** an N-player engine that plays complete games with lands, mana, vanilla and French-vanilla creatures, simple spells (hand-coded IR), combat, SBAs, deterministic replays, and a random policy.

## Checklist
- [x] `GameState`, object arena, zones ([[Game State Model]], [[Zones & Objects]])
- [x] Decision model v1: `PendingDecision`/`Choice`, continuations ([[Decision & Action Model]]) → accept ADR-0004
- [x] Turn structure and turn-based actions ([[Turn Structure]])
- [x] Priority and the stack, resolution ([[Priority & the Stack]], [[Resolution]])
- [x] Mana pools, mana abilities, cost payment v1 with plan enumeration ([[Costs & Mana]])
- [x] Casting procedure skeleton per CR 601.2 ([[Casting & Activating]])
- [x] Combat with factored declarations ([[Combat]])
- [x] Core SBAs (704.5a–j) ([[State-Based Actions]])
- [x] Mulligans (London, multiplayer free mulligan)
- [x] Seeded RNG, replay log, determinism tests → accept ADR-0005 ([[Determinism & Replay]])
- [x] PlayerView v1 with knowledge tracking basics ([[Hidden Information & Player Views]])
- [x] Event system v1 ([[Event System]])
- [x] `mtg-policy`: the Policy trait, random, passive, scripted
- [x] Sim skeleton: run N random games in parallel. Invariant checks ([[Invariants & Property Tests]])
- [x] Benchmarks: clone cost, games/sec → decide the persistent vs. arena state strategy ([[Performance & Cheap Cloning]])
- [x] Scenario file loader v1 ([[Scenario File Format]])

## Exit criteria
1M random 4-player games with vanilla decks: no panics, no invariant violations, 100% replay determinism. Benchmarks recorded. ✅ **Met on 2026-10-21.**

## Results
**Fuzz:** 1,000,000 random 4-player games on the synthetic test set (`mtg dev fuzz`, seeds 2026–2035, 100k per seed).

| Measure | Result |
|---|---|
| Decisions checked | ~4.0 billion |
| Invariant violations | **0**: all 9 invariants after every decision; view audits every decision in the first 400k, then every 16th decision plus game end |
| Replays re-run from their logs | **1,000 / 1,000** identical (every 1,000th game) |
| Panics | **0** |
| Real vanilla cards (741 convertible from the card DB) | 2,000 games, clean |

- Results are identical on 1 thread and 18 threads.
- Three golden replays (2, 4 and 6 players) are committed in `tests/golden/`.

**Benchmarks** (`cargo bench -p mtg-engine`, one core, Apple Silicon):

| Benchmark | Result | Target |
|---|---|---|
| `GameState` clone, 4-player mid-game | **377 ns** | NFR-P2 ≤ 10 µs |
| Legal-option enumeration at priority | **334 ns** | NFR-P3 ≤ 20 µs |
| Complete random 4-player game (~4,000 decisions) | **1.37 ms** | NFR-P1 ≤ 5 ms |
| State hash | 6.9 µs | — |
| Unchecked fuzz throughput | 669 games/s on one core, 8,834 games/s on 18 cores | — |

- The benchmark board had only 28 permanents, smaller than a typical cEDH board (~60). Clone is linear in object count, so the margin holds.
- **Q10 is decided:** plain `Vec` arenas; persistent structures aren't needed.

**Rules tests:**
- 16 CR-cited scenarios (`tests/scenarios/*.yaml`)
- game-start rule tests (CR 103.5c, 103.8a/c)
- random-game property tests: every offered option applies, clone-then-apply equals apply, and replays reproduce

**Decisions ready for the user:** ADR-0004 and ADR-0005 have the evidence their "accept by M2" conditions asked for (see each ADR).

## Plan (final, 2026-10-20)
ADR-0012 was accepted as E3 (clean-room core), so this plan stands. It builds in four lessons from [[phase.rs Evaluation]]:
1. Every offered option must apply successfully, and a test checks it.
2. The cost of enumerating options is a design input.
3. Policies only ever see a `PlayerView`.
4. Nothing is ever dropped silently.

### Design decisions
| # | Decision | Why |
|---|---|---|
| D1 | **Every decision is "pick one of N enumerated options".** Multi-part choices are factored into sequences: bottoming after a mulligan picks one card at a time, cleanup discards one card at a time, attacks and blocks are per creature, and damage division is assigned one point at a time. A `Choice` is `(DecisionId, option index)`. | One uniform contract for every client. Small option lists for random play, search and LLMs. Replays are compact (R1, R7). |
| D2 | **Option 0 is the "decline" option** wherever one exists (pass, keep, don't attack, don't block). | Gives a trivial passive policy and a stable convention for scripted tests. |
| D3 | **Continuations are a task stack.** `GameState` holds a `Vec<Task>` of small enums (begin step, priority, cast step, combat step…). The engine loop pops and runs tasks until one needs a decision. `apply(choice)` feeds the choice to the task on top. Q11 is resolved for M2 as hand-written enums in a stack; revisit at M4. | Cloneable and serializable mid-procedure (ADR-0004), with no async. |
| D4 | **State uses plain `Vec` arenas** (strategy A/C). Objects hold a `DefId` into an `Arc<CardPool>` shared by every clone, plus small `Copy` fields. They have no heap allocations, so a clone is mostly a memcpy. Zones are `Vec<ObjectId>`. A zone change allocates a new id (CR 400.7). | Targets NFR-P2 (≤ 10 µs clone). `im` is tried only if benchmarks miss the target (Q10). |
| D5 | **Temporary M2 card IR** (`mtg_engine::card`): P/T, a keyword bitset (flying, reach, first strike, double strike, deathtouch, lifelink, trample, vigilance, haste, defender, menace), `{T}: Add {X}` mana abilities including intrinsic basic-land abilities (CR 305.6), and spell effects (deal N damage to any target, draw N, gain N life, destroy target creature). Definitions come from a synthetic test set, or from `mtg-data` records that are vanilla, French-vanilla or basic lands. Anything else is a loud error. | Lets M2 play real vanilla decks without pre-empting ADR-0007. M3 replaces it. |
| D6 | **Mana payment** offers enumerated payment plans: assignments of pool mana and untapped mana sources to the cost, de-duplicated when they leave identical outcomes (identical sources are interchangeable), capped at 32. A "pay manually" option steps through single mana-ability activations, offering only steps that keep payment completable. | Both are legal CR behaviour (R2, Q13). Plans make random and LLM play sensible. |
| D7 | **Castability is checked exactly before offering.** M2 costs are mana-only and targets don't depend on payment, so "castable" means a target exists for every slot and at least one plan exists. CR 601.2e still runs as an assertion. | Legality by construction, with no rewind needed (Q14 is deferred to M3, where non-mana costs appear). |
| D8 | **Blocks are constraint-aware.** For menace, an option is offered only if a matching (remaining candidates → attackers that still need blockers) can still satisfy every pending constraint. | A first answer to Q12 that generalizes to requirements later. |
| D9 | **Determinism:**<br>• game RNG: ChaCha8, using our own unbiased bounded sampling and Fisher–Yates, so dependency upgrades can't change shuffles<br>• per-game seeds from splitmix64(experiment seed, game index)<br>• policy RNG: separate per-seat streams<br>• state hash: our own FNV-1a over a canonical traversal, little-endian<br>• no `HashMap` anywhere in the engine | ADR-0005, NFR-D1. |
| D10 | **A turn limit is a harness feature, not a rule.** `GameRules::turn_limit` ends the game as `GameResult::Aborted { TurnLimit }`, and it's reported separately. | Random play can stall. Results stay honest. |
| D11 | **Events:** an append-only `Vec<Event>` per game, each carrying a `CrRef`. Logging is configurable (`Off`/`On`) for throughput. The `PlayerView` exposes public events only. | [[Event System]]. |
| D12 | **Scenario files are YAML via `yaml_serde`** (maintained by the YAML organization), using the [[Scenario File Format]] shape. Options are matched by label. | Q22 is resolved for scenarios. Card-definition overrides remain M3. |

### Phases
| # | Work | CR | Done when |
|---|---|---|---|
| 2.1 | Card IR v0 and pool; `GameState`, players, zones, object arena, RNG, hash | 109, 400, 400.7, 103 | Unit tests |
| 2.2 | Decision types, task-stack engine loop, events, game setup, mulligans, game start, turn order | 103.1–103.8, 117 | A game starts, and passive policies loop through turns until a library runs out (704.5b) |
| 2.3 | Turn structure and turn-based actions (untap, draw, cleanup discard, damage removal, pools empty); priority passing, holding priority, stack resolution, land play | 500–505, 512–514, 117, 305.1, 116.2a, 405, 608 | Scripted scenarios |
| 2.4 | Mana (pools, abilities, plan enumeration, manual payment) and casting 601.2a–i for permanents and the M2 spell effects; targets with recheck on resolution | 106, 305.6, 601, 605, 608.2b | Property: every offered plan pays |
| 2.5 | Combat: factored attacks (multiple defending players), blocks (APNAP across defenders; flying/reach/menace constraints), first/double strike steps, per-point damage division with trample and deathtouch, lifelink | 506–511, 702.2, .4, .7, .9, .15, .17, .19, .20, .111 | Combat scenarios |
| 2.6 | SBAs 704.5a–j with the legend rule as a decision; losing and leaving the game; game end and draws | 704, 104, 800.4a | SBA scenarios |
| 2.7 | `PlayerView`, a redaction snapshot plus an auditor test; `mtg-policy` (`Policy` trait, random with pass bias, passive, scripted) | 400.2 | The auditor finds no hidden identities |
| 2.8 | Replay log and replay runner (first-divergence report); invariant checker (9 invariants); the "every option applies" property test | — | Determinism tests → accept ADR-0004/0005 |
| 2.9 | `mtg-sim` runner (rayon, per-game seeds, stats) and `mtg dev fuzz`; synthetic vanilla decks plus real vanilla decks from the card DB | — | **1M 4-player games**, exit criterion |
| 2.10 | criterion benchmarks: clone, random game, priority enumeration; record results in [[Non-Functional Requirements]] | — | Numbers recorded, Q10 decided |
| 2.11 | Scenario loader v1 (YAML) plus CR-cited scenario tests for each phase | — | Scenarios run from files |

### Verification at the end of M2
- `cargo nextest run`, clippy and fmt are clean.
- `mtg dev cr-check` passes.
- `mtg dev fuzz --games 1000000 --players 4 --invariants --replay-every 1000` shows zero panics, zero violations and zero replay divergences.
- Benchmark numbers are recorded.
