---
type: requirements
status: draft
tags: [requirements, performance]
updated: 2026-10-20
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

## Related
[[Functional Requirements]] · [[Performance & Cheap Cloning]] · [[Batch Runner & Parallelism]]
