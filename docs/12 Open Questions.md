---
type: index
status: living
tags: [open-questions]
updated: 2026-10-20
---

# Open Questions

Each question has an owner note and a target milestone. Move a question to **Resolved** with a link to the decision.

## Needs the user
| # | Question | Notes | Target |
|---|---|---|---|
| Q1 | **Project name?** "mtg-rules-engine" is a working title. | Avoid WotC trademarks in the name (e.g. "Magic"). | M0 |
| Q2 | Review the proposed ADRs: 0004 and 0005 (M2 evidence: ready to accept), 0009 (M1 evidence: ready to accept), 0008 and 0011 (M7/M8). Accept or amend? | [[ADR Index]] | now |
| Q3 | Create the GitHub repo (public or private) and `git init`? | [[M0 Groundwork]] | M0 |
| Q4 | Should deck *optimization* (suggesting swaps from sims) become a goal later? | Currently a non-goal ([[Scope & Non-Goals]]) | post-M9 |
| Q5 | Arena-only digital mechanics (Alchemy) ever? | Non-goal for now | — |

## Architecture
| # | Question | Owner note | Target |
|---|---|---|---|
| Q10 | ~~Persistent vs. arena-clone state~~ **Decided in M2:** plain `Vec` arenas (clone 377 ns, far under the 10 µs target). Revisit only if real cEDH boards change that. | [[Performance & Cheap Cloning]] | done |
| Q12 | Constraint-aware factoring for attack/block requirements (CR 508.1d / 509.1c). **M2: done for menace** (block options are offered only if a matching keeps every menace attacker satisfiable). Requirements and restrictions ("attacks each combat if able") remain. | [[Combat]], [[Decision & Action Model]] | M4 |
| Q14 | Precise castability pre-check vs. lazy check with invisible rewind (CR 601.2e / 733). **M2: precise pre-check**, which is exact for mana-only costs. Non-mana costs and cost modifiers in M3 will decide it. | [[Casting & Activating]] | M3 |
| Q15 | Representation of arbitrarily large numbers (FR-E11) | [[Loops & Shortcuts]], [[MTG Complexity & Loops]] | M4 |
| Q16 | Loop detection: hashing "modulo monotonic quantities", and proving linearity for extrapolation | [[Loops & Shortcuts]] | M4 |
| Q17 | How much last-known information to snapshot. M2 needs none (no triggers or abilities look back), so this is deferred. | [[Zones & Objects]] | M4 |

## Cards
| # | Question | Owner note | Target |
|---|---|---|---|
| Q20 | **Card definition approach.** Spike done ([[Card Definition Spike Results]]); D (hybrid) recommended. **Awaiting the user's decision.** | [[ADR-0007 Card Definition Approach]] | M3 |
| Q23 | "Spell commander" (5 cards) isn't in the 2026-06-19 CR. M1 treats it as CR 903.3a "can be your commander" text. Revisit when the CR adds it. | [[Commander]], [[CR Update Process]] | next CR |
| Q24 | Use phase.rs's per-card coverage as a differential oracle? The spike wires it into `mtg dev card-spike --phase-coverage`; the recommendation is yes, for flagging disagreements only. | [[phase.rs Evaluation]] | M3 |
| Q22 | DSL syntax for overrides. Spike evidence: the IR's own YAML form (single-key maps, strict schema) worked for 40 cards. Decided with ADR-0007. | [[Option - Hybrid IR]] | M3 |

## Simulation & policies
| # | Question | Owner note | Target |
|---|---|---|---|
| Q30 | Playbook language: Rhai vs. Lua vs. declarative YAML | [[B - Playbooks]] | M8 |
| Q31 | Multiplayer search backup rule (max^n vs. paranoid vs. other) | [[D - Search AI]] | M9 |
| Q32 | Should `open_decklists` default to true? | [[Hidden Information & Player Views]] | M7 |
| Q33 | Where run artifacts live for remote MCP clients | [[AI-Agent Integration Research]] | M8 |
| Q34 | Observation encoding for the Python RL environment | [[Python Package]] | M12 |
| Q35 | EDHTop16 data terms for bulk decklist import | [[cEDH Ground-Truth Data]] | M11 |
| Q36 | Ingest Scryfall `oracle_tags` (functional categories) for per-card analytics; deferred from M1 | [[Card Data Sources]], [[Metrics Catalog]] | M7 |

## Resolved
| # | Question | Resolution |
|---|---|---|
| R1 | Language | Rust ([[ADR-0001 Rust as Engine Language]]) |
| R2 | Who plays simulated games? | All four policy kinds, composable ([[ADR-0006 Pluggable Decision-Makers]]) |
| R3 | Primary format | Commander-first ([[ADR-0010 Commander-First N-Player Core]]) |
| R4 | License | MIT/Apache ([[ADR-0002 Licensing and Card Data Distribution]]) |
| R5 | Where docs live | `docs/` vault ([[ADR-0003 Docs as Obsidian Vault in Repo]]) |
| R6 | Binary card DB format | `postcard` with a magic+version header; content-hashed. ~210 ms load ([[M1 Foundations & Data]]) |
| R7 | Card name collisions (prepare/adventure spell faces named like real cards) | Two-tier name index: full names before face names; spell faces of adventure/prepare cards aren't indexed ([[Ingestion Pipeline]]) |
| R8 | Which color identity to validate decks with | Union of Scryfall's and the CR 903.4 recomputation, plus a CR 903.5d basic-land-type check ([[Deck Import]]) |
| R9 | Relationship to phase.rs (Q21) | E3, borrow selectively; clean-room engine core ([[ADR-0012 Relationship to phase.rs]], accepted 2026-10-20) |
| R10 | Continuations (Q11) | A stack of small hand-written task enums stored in the game state; the loop pops tasks until one needs a decision. Cloneable and hashable mid-procedure. Revisit at M4 if triggers and replacements make it unwieldy ([[M2 Core Game Loop]]) |
| R11 | Payment plans (Q13) | Outcome-distinct plans (interchangeable sources grouped), capped at 32, plus a manual mode that offers only steps keeping the payment completable ([[M2 Core Game Loop]]) |
| R12 | Scenario file syntax (part of Q22) | YAML via `yaml_serde`, matched by option label ([[Scenario File Format]]). Card-definition overrides stay open under Q22 |
