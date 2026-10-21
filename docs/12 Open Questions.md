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
| Q2 | Review the proposed ADRs 0004, 0005, 0008, 0009 and 0011: accept or amend? | [[ADR Index]] | M0 |
| Q3 | Create the GitHub repo (public or private) and `git init`? | [[M0 Groundwork]] | M0 |
| Q4 | Should deck *optimization* (suggesting swaps from sims) become a goal later? | Currently a non-goal ([[Scope & Non-Goals]]) | post-M9 |
| Q5 | Arena-only digital mechanics (Alchemy) ever? | Non-goal for now | — |

## Architecture
| # | Question | Owner note | Target |
|---|---|---|---|
| Q10 | Persistent (`rpds`/`im`) vs. arena-clone vs. hybrid game state | [[Performance & Cheap Cloning]] | M2 |
| Q11 | Hand-written continuation enums vs. a small internal "procedure VM" for rules procedures | [[Game State Model]] | M2 |
| Q12 | Constraint-aware factoring for attack/block requirements (CR 508.1d / 509.1c) | [[Combat]], [[Decision & Action Model]] | M2–M4 |
| Q13 | Payment-plan enumeration and canonicalization; when to require manual payment | [[Costs & Mana]] | M2 |
| Q14 | Precise castability pre-check vs. lazy check with invisible rewind (CR 601.2e / 733) | [[Casting & Activating]] | M2–M3 |
| Q15 | Representation of arbitrarily large numbers (FR-E11) | [[Loops & Shortcuts]], [[MTG Complexity & Loops]] | M4 |
| Q16 | Loop detection: hashing "modulo monotonic quantities", and proving linearity for extrapolation | [[Loops & Shortcuts]] | M4 |
| Q17 | How much last-known information to snapshot | [[Zones & Objects]] | M2 |

## Cards
| # | Question | Owner note | Target |
|---|---|---|---|
| Q20 | **Card definition approach** | [[ADR-0007 Card Definition Approach]] | M3 |
| Q21 | **Depend on, fork, borrow from, or ignore phase.rs** | [[ADR-0012 Relationship to phase.rs]] | M1 |
| Q22 | DSL syntax (YAML/RON/custom) if overrides exist | [[Option - Hybrid IR]] | M3 |

## Simulation & policies
| # | Question | Owner note | Target |
|---|---|---|---|
| Q30 | Playbook language: Rhai vs. Lua vs. declarative YAML | [[B - Playbooks]] | M8 |
| Q31 | Multiplayer search backup rule (max^n vs. paranoid vs. other) | [[D - Search AI]] | M9 |
| Q32 | Should `open_decklists` default to true? | [[Hidden Information & Player Views]] | M7 |
| Q33 | Where run artifacts live for remote MCP clients | [[AI-Agent Integration Research]] | M8 |
| Q34 | Observation encoding for the Python RL environment | [[Python Package]] | M12 |
| Q35 | EDHTop16 data terms for bulk decklist import | [[cEDH Ground-Truth Data]] | M11 |

## Resolved
| # | Question | Resolution |
|---|---|---|
| R1 | Language | Rust ([[ADR-0001 Rust as Engine Language]]) |
| R2 | Who plays simulated games? | All four policy kinds, composable ([[ADR-0006 Pluggable Decision-Makers]]) |
| R3 | Primary format | Commander-first ([[ADR-0010 Commander-First N-Player Core]]) |
| R4 | License | MIT/Apache ([[ADR-0002 Licensing and Card Data Distribution]]) |
| R5 | Where docs live | `docs/` vault ([[ADR-0003 Docs as Obsidian Vault in Repo]]) |
