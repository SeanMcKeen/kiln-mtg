---
type: requirements
status: draft
tags: [requirements]
updated: 2026-10-20
---

# Functional Requirements

IDs are stable so that ADRs, milestones and tests can reference them. **MUST** / **SHOULD** / **MAY** follow RFC 2119.

## Rules engine (FR-E)
| ID | Requirement |
|---|---|
| FR-E1 | MUST implement the CR as published (baseline 2026-06-19) for supported formats, citing rule numbers. |
| FR-E2 | MUST never accept an illegal action. Only legal options are offered ([[Decision & Action Model]]). |
| FR-E3 | MUST expose every player choice permitted by the CR as an explicit decision (see [[Design Principles]] §2). |
| FR-E4 | MUST support 2–N players (N ≥ 6, design target 8), including players leaving the game (CR 800.4a). |
| FR-E5 | MUST be deterministic given (card DB version, setup, seed, choice log) ([[Determinism & Replay]]). |
| FR-E6 | MUST produce redacted per-player views. Hidden zones are hidden unless revealed ([[Hidden Information & Player Views]]). |
| FR-E7 | MUST support cheap state cloning for search ([[Performance & Cheap Cloning]]). |
| FR-E8 | MUST support CR 732 shortcuts and loop detection, including mandatory-loop draws (CR 104.4b, 732.4) ([[Loops & Shortcuts]]). |
| FR-E9 | MUST emit a structured event stream for metrics, logs and explanations ([[Event System]]). |
| FR-E10 | SHOULD be able to explain *why* an option is unavailable ("Grand Abolisher: you can't cast spells during this player's turn"), for the judge tool. |
| FR-E11 | MUST support arbitrarily large numbers produced by infinite combos without overflow UB. The strategy is an open question ([[12 Open Questions]]). |

## Cards & data (FR-C)
| ID | Requirement |
|---|---|
| FR-C1 | MUST ingest Oracle card data from Scryfall and/or MTGJSON without redistributing it ([[Ingestion Pipeline]]). |
| FR-C2 | MUST track per-card coverage status (`unsupported`/`parsed`/`verified`) ([[Card Data Model]]). |
| FR-C3 | MUST validate deck legality per format (including Commander color identity, CR 903.4–903.5). |
| FR-C4 | MUST version the card database and stamp it into every run report. |
| FR-C5 | SHOULD import decklists from MTGO/Arena text, Moxfield and Archidekt ([[Deck Import]]). |

## Simulation (FR-S)
| ID | Requirement |
|---|---|
| FR-S1 | MUST run N-seat experiments from a declarative spec ([[Experiment Spec]]). |
| FR-S2 | MUST allow a different policy per seat and composed policies ([[Policy Composition]]). |
| FR-S3 | MUST support seating strategies: fixed, random, balanced rotation ([[Statistics for Multiplayer Simulation]]). |
| FR-S4 | MUST run games in parallel with results independent of thread scheduling ([[Batch Runner & Parallelism]]). |
| FR-S5 | MUST let the caller toggle every metric category independently ([[Metrics Catalog]]). |
| FR-S6 | MUST report confidence intervals for every rate metric. |
| FR-S7 | MUST be able to retain full replays (all, sampled, or "interesting" games) ([[Reproducibility]]). |
| FR-S8 | MUST refuse to run, or loudly flag the run, when any deck contains non-`verified` cards (configurable) ([[Validity & Caveats]]). |
| FR-S9 | SHOULD support paired A/B variants with common random numbers. |
| FR-S10 | SHOULD support goldfish and opening-hand-only modes. |
| FR-S11 | MUST cap game length (turns, decisions, wall time) and record the reason when a cap is hit. |

## Policies (FR-P)
| ID | Requirement |
|---|---|
| FR-P1 | MUST define one `Policy` interface used by all decision-makers ([[Policy Interface]]). |
| FR-P2 | MUST ship random-legal and greedy-heuristic baselines ([[A - Baseline Policies]]). |
| FR-P3 | MUST run sandboxed playbook scripts with resource limits ([[B - Playbooks]]). |
| FR-P4 | MUST support an external/LLM policy through a request/response protocol ([[C - LLM Live Decisions]]). |
| FR-P5 | MUST provide a search policy with configurable budget ([[D - Search AI]]). |
| FR-P6 | Policies MUST NOT access hidden information beyond their player view. |

## Interfaces (FR-I)
| ID | Requirement |
|---|---|
| FR-I1 | MUST expose a stable Rust library API ([[Library API]]). |
| FR-I2 | MUST expose a CLI with machine-readable JSON output ([[CLI & REPL]]). |
| FR-I3 | MUST expose an MCP server covering the headline use case ([[MCP Server Tool Catalog]]). |
| FR-I4 | SHOULD expose Python bindings ([[Python Package]]) and MAY expose WASM ([[WASM Build]]). |
| FR-I5 | MUST provide agent-facing documentation sufficient to run US-1 unaided ([[Agent-Facing Docs Plan]]). |

## Judge (FR-J)
| ID | Requirement |
|---|---|
| FR-J1 | MUST allow constructing arbitrary legal board states from a scenario file ([[Scenario File Format]]). |
| FR-J2 | MUST step through actions and show the event log with CR citations. |
| FR-J3 | SHOULD answer "why is X illegal?" (see FR-E10). |

## Related
[[Non-Functional Requirements]] · [[User Stories]] · [[Formats & Game Modes]]
