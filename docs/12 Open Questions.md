---
type: index
status: living
tags: [open-questions]
updated: 2026-10-27
---

# Open Questions

Each question has an owner note and a target milestone. Move a question to **Resolved** with a link to the decision.

## Needs the user
| # | Question | Notes | Target |
|---|---|---|---|
| Q2 | Review ADR-0008 and ADR-0011 when M7/M8 produce evidence (0004, 0005, 0007 and 0009 were accepted on 2026-10-21). | [[ADR Index]] | M7 |
| Q4 | Should deck *optimization* (suggesting swaps from sims) become a goal later? | Currently a non-goal ([[Scope & Non-Goals]]) | post-M9 |
| Q5 | Arena-only digital mechanics (Alchemy) ever? | Non-goal for now | — |

## Architecture
| # | Question | Owner note | Target |
|---|---|---|---|
| Q38 | **Parquet tables.** M7 writes run tables as JSONL and CSV (M7 plan S12): the `arrow`/`parquet` crates add hundreds of dependencies and long compiles, and DuckDB, polars and pandas read JSONL directly (DuckDB: `COPY (SELECT * FROM 'games.jsonl') TO 'games.parquet'`). Add native Parquet behind a cargo feature when runs get large enough that JSONL size or load time hurts? | [[Output Formats & Report Schema]] | M11 |
| Q37 | **Optional loops that change nothing.** Re-equipping Lightning Greaves for {0}, or Basalt Monolith paying {3} to untap after making {C}{C}{C}, repeat a state with no monotonic change, so no shortcut is offered (CR 732.2a needs something to repeat) and nothing stops the loop but the player. M6 leaves this to policies (the greedy baseline activates each ability once per turn) and the fuzz harness reports games past `--decision-limit`. Should the engine end such loops, as tournament rules do for loops without progress, for every policy (M9 search, M10 LLM)? | [[Loops & Shortcuts]] | M7 |
| Q10 | ~~Persistent vs. arena-clone state~~ **Decided in M2:** plain `Vec` arenas (clone 377 ns, far under the 10 µs target). Revisit only if real cEDH boards change that. | [[Performance & Cheap Cloning]] | done |
| Q12 | Constraint-aware factoring for attack/block requirements (CR 508.1d / 509.1c). **M2: done for menace** (block options are offered only if a matching keeps every menace attacker satisfiable). Requirements and restrictions ("attacks each combat if able") remain. | [[Combat]], [[Decision & Action Model]] | M4 |
| Q29 | **Leaving mid-procedure.** A concession while another player's procedure is under way (e.g. choosing targets) can leave that procedure with no legal option; the engine ends the procedure, where CR 733 would rewind it. Choices an effect needs from a player who left are skipped rather than handed to another player (CR 800.4g–h). The fuzzer only has the deciding player concede, which never hits these. | [[Multiplayer]] | M7 |
| Q28 | **CR 903.9b and other draw replacements.** When a commander is on top of its owner's library and another replacement would apply to the draw (Notion Thief), CR 616.1 lets the owner choose the order. M6 asks 903.9b only once the draw actually happens (after other draw replacements were applied or declined). Also open: two mandatory replacements with identical results (Rest in Peace + Leyline of the Void) technically form a CR 616.1 choice with no observable difference. | [[Commander]], [[Replacement & Prevention]] | M6 6.5 |

## Cards
| # | Question | Owner note | Target |
|---|---|---|---|
| Q23 | "Spell commander" (5 cards) isn't in the 2026-06-19 CR. M1 treats it as CR 903.3a "can be your commander" text. Revisit when the CR adds it. | [[Commander]], [[CR Update Process]] | next CR |
| Q25 | **Where do real-card scenarios run?** Verified-card scenarios need characteristics (cost, type line, P/T) from the card DB, which isn't committed. M3 runs them only when `data/cache/current` or `KILN_CARD_DB` exists, so CI covers synthetic cards plus the real scenarios' CR citations (109 scenarios, 123 cards pass locally). Option: a CI job that downloads Scryfall bulk data at CI time, like the CR check does (nothing committed, but it adds network use and Scryfall's terms apply). **Recommendation:** do that once a repository with CI exists (Q3), caching the download per day. Until then, local runs only. | [[M3 Card Definitions v1]], [[Card Data Sources]] | with Q3 |

## Simulation & policies
| # | Question | Owner note | Target |
|---|---|---|---|
| Q40 | **MCP over Streamable HTTP.** M8 serves MCP over stdio only. `rmcp`'s Streamable HTTP server needs an HTTP stack (axum/hyper/tower). Add it behind a cargo feature when remote agents need it? | [[MCP Server Tool Catalog]] | M10 |
| Q39 | **No seat-1 advantage with baseline pilots.** In US-1 (5,000 greedy games) seat 1 wins 24.2% and seat 4 25.6%, while real cEDH shows a strong first-seat edge ([[Statistics for Multiplayer Simulation]]). Greedy games run long (median 46 turns), so turn order washes out. Reproducing the seat-1 advantage should be one of M9's play-quality checks ([[Measuring Play Quality]]). | [[Measuring Play Quality]] | M9 |
| Q31 | Multiplayer search backup rule (max^n vs. paranoid vs. other) | [[D - Search AI]] | M9 |
| Q32 | Should `open_decklists` default to true? | [[Hidden Information & Player Views]] | M7 |
| Q33 | Where run artifacts live for remote MCP clients | [[AI-Agent Integration Research]] | M8 |
| Q34 | Observation encoding for the Python RL environment | [[Python Package]] | M12 |
| Q35 | EDHTop16 data terms for bulk decklist import | [[cEDH Ground-Truth Data]] | M11 |
| Q36 | Ingest Scryfall `oracle_tags` (functional categories) for per-card analytics; deferred from M1 | [[Card Data Sources]], [[Metrics Catalog]] | M7 |

## Resolved
| # | Question | Resolution |
|---|---|---|
| Q41 | Turn limits in own turns? | Kept in game turns (they match the event log and the CR's turn numbers). The default `max_turns: 100` is 25 own turns in a pod; the 60-turn override in the goldfish example was the problem and is gone (Najeela goldfish capped 32.9% → 1.4%). The capped warning now states the limit in turns per player. The fastest-win hint ranks by the winner's own turn and puts wins by opponents decking last. |
| Q30 | Playbook language | Rhai (M8): pure Rust, a built-in sandbox with limits, deterministic, JS-like syntax. See [[B - Playbooks]]. |
| Q26 | Loops that move cards between zones | M6 6.7d: loop identity hashes object ids by arena slot (`kiln_types::hash_ids_by_slot`, CR 400.7), without object timestamps or attachments of departed objects. Worldgorger Dragon + Animate Dead is detected as a mandatory loop and drawn (T12, CR 104.4b). |
| Q27 | An Aura entering without being cast chooses its host | M6 6.7d: it chooses as it enters (CR 303.4f; a `choose_card` decision with purpose `Enchant`, automatic with one legal object) and stays in its zone with none (303.4g). |
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
| R13 | Castability: precise pre-check vs. lazy check with rewind (Q14) | **Precise pre-check.** M3 extends it to modes, alternative and additional costs, X, hybrid and Phyrexian symbols: each casting choice is offered only while a complete cast still exists, so CR 601.2e never fails and no rewind (CR 733) is needed. Mana and other costs are checked independently, which is exact because mana is paid first (CR 601.2g–h). Revisit if cost modifiers in M5 break independence ([[M3 Card Definitions v1]]) |
| R12 | Scenario file syntax (part of Q22) | YAML via `yaml_serde`, matched by option label ([[Scenario File Format]]). Card-definition overrides stay open under Q22 |
| R14 | Card definition approach (Q20) | **D, hybrid**: parser first, strict YAML overrides, named Rust escapes, mandatory renderer lint ([[ADR-0007 Card Definition Approach]], accepted 2026-10-21) |
| R15 | DSL syntax for overrides (Q22) | The IR's own strict YAML form (single-key maps, `deny_unknown_fields`), as in `cards/defs/` (ADR-0007) |
| R17 | Large numbers (Q15, FR-E11) | Stay `i64`/`u32` with saturating arithmetic; a shortcut is at most 10⁹ iterations (M4 D10) ([[M4 Triggers & Replacement]]) |
| R18 | Loop detection (Q16) | Hash the state minus life, poison, mana pools and counter counts at each priority decision once a step is busy (16+). On a repeat, the per-iteration delta is applied N times exactly, so no extrapolation proof is needed. Only one player making choices → `ProposeShortcut`, then the others accept or shorten. Nobody making choices → a mandatory loop, run to the losing iteration or drawn if endless (`loops.rs`, M4 4.7) |
| R19 | Last-known information (Q17) | `ZoneChange` events carry controller, power and toughness. Effects record LKI as they move objects; triggers snapshot it (M4 4.1) |
| R16 | phase.rs coverage as a differential oracle (Q24) | Yes, for flagging disagreements only; it never marks a card supported ([[phase.rs Evaluation]], accepted with ADR-0007) |
| R20 | Project name (was Q1) | Kiln; crate `kiln-mtg`, binary `kiln` ([[ADR-0013 Project Name]]) |
| R21 | GitHub repo (was Q3) | Public: [SeanMcKeen/kiln-mtg](https://github.com/SeanMcKeen/kiln-mtg) ([[M0 Groundwork]]) |
