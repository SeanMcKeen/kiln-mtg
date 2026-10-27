---
type: requirements
status: draft
tags: [requirements, performance]
updated: 2026-10-27
---

# Non-Functional Requirements

> [!note] These are targets, not measurements
> These numbers are initial targets that get re-baselined once benchmarks exist (M2, M7, M9). They come from the budget math below, not from measurement. See [[Performance & Cheap Cloning]].

## Throughput: what makes "5,000 cEDH games" practical
A rough model is `wall time ≈ games × decisions_per_game × time_per_decision / cores`.

Assumptions for a 4-player cEDH game:
- about **150–400 policy decisions**, counting priority passes (most of which are trivial)
- an 8-core machine

| Policy mix | Time per decision | Per game | 5,000 games on 8 cores |
|---|---|---|---|
| Random / greedy baselines | ~2–10 µs (engine-bound) | ~1–5 ms | **~1–5 s** |
| Playbook (Rhai) + greedy fallback | ~20–100 µs | ~10–40 ms | **~10–30 s** |
| Search AI, 10 ms budget on non-trivial decisions (~60/game) | 10 ms | ~0.6 s | **~6 min** |
| Search AI, 50 ms budget | 50 ms | ~3 s | **~30 min** |
| Live LLM (1 seat) | 2–10 s | 10–40 min | **not viable**. Spot checks only (~20 games). |

Targets that fall out of this:
| ID | Target |
|---|---|
| NFR-P1 | Engine overhead, excluding policy time, ≤ **5 ms per 4-player cEDH game** on one modern core by M7. |
| NFR-P2 | `GameState` clone ≤ **10 µs** at typical cEDH board size (≤ 150 objects in play/visible). Persistent structures may go lower. |
| NFR-P3 | Legal-option enumeration for a priority decision ≤ **20 µs** typical. |
| NFR-P4 | Near-linear scaling across cores for batch runs (≥ 85% efficiency at 8 cores). |
| NFR-P5 | The search AI respects its time/iteration budget within ±10%, and has a deterministic **iteration** budget mode for reproducibility. |
| NFR-P6 | Card database load ≤ 1 s for the full pool, from a precompiled binary artifact. |
| NFR-P7 | Memory ≤ 64 MB per worker thread excluding the shared card DB. The card DB is shared read-only across threads. |

## Correctness
| ID | Target |
|---|---|
| NFR-C1 | Zero known rules bugs in `verified` cards. Bugs demote a card to `parsed` until fixed. |
| NFR-C2 | Every subsystem has CR-cited scenario tests. Fuzzing via random play runs in CI with invariant checks ([[Invariants & Property Tests]]). |
| NFR-C3 | Golden replays never change silently. Intentional changes require a re-bless with a reason ([[Golden Replays]]). |

## Determinism & reproducibility
| ID | Target |
|---|---|
| NFR-D1 | Same inputs give bit-identical reports across runs, thread counts, and OS (x86_64/aarch64). |
| NFR-D2 | Every report embeds: engine version, card DB version (data date and hash), CR version, spec hash, seed. |
| NFR-D3 | No dependence on `HashMap` iteration order, wall-clock time, or thread scheduling in game logic. |

## Usability for agents
| ID | Target |
|---|---|
| NFR-U1 | The concise report fits in about 2k tokens. The detailed report is paginated. Every response is capped at 25k tokens ([[AI-Agent Integration Research]]). |
| NFR-U2 | Every error is actionable: what went wrong, plus an example of a valid input. |
| NFR-U3 | Long runs report progress and an ETA, and can be cancelled. |

## Maintainability
| ID | Target |
|---|---|
| NFR-M1 | A new CR release is integrated within 2 weeks of publication ([[CR Update Process]]). |
| NFR-M2 | New-set cards are triaged automatically: the parser attempts them and coverage reports the diff. |
| NFR-M3 | `cargo clippy -D warnings`, `cargo fmt`, and the test suite all gate merges. |

## Portability
- Tier 1: macOS aarch64, Linux x86_64.
- Tier 2: Windows x86_64, `wasm32-unknown-unknown` (engine and sim, without threads).

## Measurements
| Date | Target | Measured | Conditions |
|---|---|---|---|
| 2026-10-21 (M2) | NFR-P1 ≤ 5 ms engine time per 4-player game | **1.37 ms** per complete random game | Synthetic vanilla decks, random policy, one core, Apple Silicon; `cargo bench -p kiln-engine` |
| 2026-10-21 (M2) | NFR-P2 clone ≤ 10 µs | **377 ns** | 4-player mid-game, 28 permanents (smaller than a cEDH board) |
| 2026-10-21 (M2) | NFR-P3 priority enumeration ≤ 20 µs | **334 ns** | Same board |
| 2026-10-21 (M1) | NFR-P6 card DB load ≤ 1 s | **~210 ms** | Full Scryfall `oracle_cards` (38,940 objects) |
| 2026-10-21 (M2) | NFR-P4 ≥ 85% scaling at 8 cores | 13.2× on 18 cores (73%; mix of performance and efficiency cores) | `kiln dev fuzz`, unchecked |
| 2026-10-21 (M2) | NFR-D1 identical across thread counts | ✅ identical results on 1 and 18 threads | 5,000-game batch |
| 2026-10-21 (M3) | — (throughput with real cards) | 475 games/s, 4-player, ~2,100 decisions/game | Verified-card decks, random policy, 2 threads, invariants + view audits + replays on (`kiln dev fuzz --verified`) |
| 2026-10-23 (M5) | NFR-P1 ≤ 5 ms per 4-player game | **4.19 ms** per complete random game (pre-M5: 4.11 ms) | Same synthetic decks and policy; triggers, replacements and layers on |
| 2026-10-23 (M5) | NFR-P2 clone ≤ 10 µs | **665 ns** | Same board |
| 2026-10-23 (M5) | NFR-P3 priority enumeration ≤ 20 µs | **1.46 µs** (pre-M5: 1.04 µs) | Same board |
| 2026-10-23 (M5) | Characteristic recomputation ≤ 20 µs (M5 exit) | **19.1 µs** | 60 permanents, 10 layer statics, full recompute (`characteristics, recomputed`) |
| 2026-10-23 (M5) | — (state hash) | 13.8 µs (pre-M5: 11.1 µs) | Same board |

| 2026-10-27 (M6) | NFR-P1 ≤ 5 ms per 4-player game | **22 ms** per random-policy game (104 turns, 4,275 decisions: ~5.2 µs per decision); **17 ms** per greedy game (48 turns, 2,327 decisions) | The four M6 test decks (100 verified cards each, real commanders), one core, Apple Silicon, checks off, policy time included (`kiln dev fuzz --verified --decks … --threads 1`) |

The M2 → M5 rise in per-game time (1.37 → 4.19 ms) leaves little NFR-P1 headroom; profiling is scheduled for M7.

M6 re-baseline with real cEDH-style decks: NFR-P1 is **not met** as measured. Random and greedy games run far longer than real cEDH games (often ten turns or fewer), so per-decision cost (~5 µs including the policy) is the more useful figure until M7 profiling. A game of ~1,000 decisions would land near the 5 ms target.

## Related
[[Functional Requirements]] · [[Performance & Cheap Cloning]] · [[Batch Runner & Parallelism]]
