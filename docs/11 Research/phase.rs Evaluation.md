---
type: research
status: complete
tags: [research, prior-art, phase-rs, decision]
updated: 2026-10-20
---

# phase.rs Evaluation

## Question
Should we depend on (E1), fork (E2), borrow selectively from (E3), or ignore (E4) [phase.rs](https://github.com/phase-rs/phase)? See [[ADR-0012 Relationship to phase.rs]]. This is the time-boxed M1 evaluation against the ADR's five criteria: decision-model fit, hidden information, throughput, parser quality on the [[Curated cEDH Core Pool]], and maintainability.

## Method
- Shallow clone of `main` at commit `7b1504a` (2026-10-20), kept outside our repo.
- Built `phase-engine` in release mode on its pinned toolchain (`nightly-2026-04-19`) on an Apple Silicon Mac.
- Read the decision types (`WaitingFor`, `GameAction`), the legal-action generator, visibility filtering, AI determinization, RNG, and replay.
- Wrote an out-of-tree bench crate against its public API:
  - 4-player Commander games, a uniformly random choice from `ai_support::legal_actions`, played until game over
  - decks: the first four bracket-free MTGGoldfish Commander decks in its own feed (not cEDH lists)
  - recorded throughput, clone cost, enumeration cost, and offered actions the engine then rejected
- Card coverage: compared its published per-card coverage report (`coverage-data.json`, generated 2026-10-19 from MTGJSON) against the 205 names in our curated cEDH pool, and spot-checked parse trees of hard cards.

## Findings

### 1. Decision model: close in spirit, weaker in guarantees
- **The same architecture we proposed.** It's an explicit state machine: `GameState.waiting_for: WaitingFor` (well over 100 decision variants, CR-annotated) is answered with a `GameAction`. It's cloneable and serializable mid-decision, matching [[ADR-0004 Engine as Decision State Machine]]. It exposes trigger ordering, replacement choice, scry/surveil ordering, mode choice and more.
- **The legal set is advisory.** `legal_actions` lives in `ai_support`, and the engine validates on `apply`. In 20 random games (49,662 actions), **39 offered actions were rejected**:
  - `Mulligan` offered while the player still owed bottom cards
  - `CastSpell` offered when an additional cost couldn't be paid

  That breaks our Design Principle 1, legality by construction.
- **Declarations aren't factored.** `DeclareAttackers` is one joint decision with per-attacker support sets. Our R1 asks one creature at a time.
- **Mana payment is auto by default.** `CastPaymentMode::Auto` picks a payment for "unambiguous" costs, with `Manual` available. Our R2 enumerates outcome-distinct plans instead of picking one.

### 2. Hidden information: redaction exists, but the AI cheats by default
- `filter_state_for_viewer` redacts state for clients (used by the multiplayer server).
- The AI searches the **authoritative** state. Determinization (`phase-ai/src/determinize.rs`) is opt-in: `determinization_samples` defaults to **0** in every shipped difficulty config. That conflicts with Design Principle 5 (honest information boundaries). It's fixable by configuration, but the safe behaviour isn't structural.

### 3. Throughput: about three orders of magnitude off our targets
| Measure (random policy, 4-player Commander) | phase.rs | Our target |
|---|---|---|
| `GameState` clone at ~311 objects (`im` structural sharing) | **8.9 µs** | ≤ 10 µs (NFR-P2) ✅ |
| `legal_actions` per call | **~4.0 ms** | ≤ 20 µs (NFR-P3) |
| `apply` per action (`apply_as_current`) | **~0.76 ms** | (inside NFR-P1) |
| `apply` per action (`apply_as_current_for_simulation`, display work deferred) | **~0.62 ms** | (inside NFR-P1) |
| Games per second, one core (either apply mode) | **0.08** (~12 s/game, ~2,500 actions) | ≤ 5 ms engine time per game (NFR-P1) |
| Card DB load | 0.66 s (100 MB JSON) | ≤ 1 s (NFR-P6) ✅ |
| Robustness | 20/20 games finished; no panics or stalls | — |
| Determinism | Both runs produced exactly 49,662 actions and 39 rejections from the same seeds | ✅ |

Caveats:
- one machine, default allocator (its own bins use mimalloc), random policy, non-cEDH decks
- `legal_actions` also computes effective costs for UI use
- a lighter enumeration path could cut the gap, but not by 200×

At about 12 s per random game, the headline workload of 5,000 games with baseline policies (target: seconds) would take about 17 core-hours.

### 4. Parser and card coverage: excellent, the most valuable asset
- **Self-reported coverage:** 31,954 of 35,925 cards (88.9%) and 30,500 of 32,950 Commander cards (92.6%). "Supported" means the parser found no gaps; it doesn't mean the card was verified by tests.
- **Curated cEDH pool:** **201 of 205 cards parse without gaps.** The exceptions:
  - Tivit, Seller of Secrets: the "starting with you" voting order
  - Twinflame
  - Jeweled Lotus
  - Talion, the Kindly Lord, which only looked like a gap because of a name-extraction issue on our side; it parses fine
- **Parse trees look faithful** for Necropotence (skip-draw static, discard trigger, face-down exile with a delayed return), Mana Vault (doesn't untap, optional upkeep payment, conditional draw-step damage), Mox Diamond (ETB replacement with optional discard), Opposition Agent (control during search plus a search replacement), Food Chain (restricted mana scaled by mana value) and Talion (choose a number as it enters).

### 5. Maintainability and fit
- **Size:** the `engine` crate alone is ~2,600 files and ~1.68M non-test lines. `game_state.rs` is 49k lines.
- **Toolchain and build:** a pinned nightly with the Cranelift codegen backend; a cold release build takes about 6 minutes.
- **Velocity:** the 200 most recent commits all fall within 30 days, and about 1.5k issues are open. The project uses LLM contributors heavily (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, an `$engine-implementer` skill).
- **Goals differ:** it's built client-first (Tauri, PWA, multiplayer server, React UI), while we're simulation- and agent-first.
- **Fail-loudly gap:** `resolve_deck_list` **silently skips names it can't resolve**. In our bench, a deck whose feed name carried a set suffix lost 88 of its 99 cards without an error. Design Principle 9 forbids that.
- **License:** MIT OR Apache-2.0, identical to ours, so borrowing code is legally simple (keep attribution).

## Implications for us
| Option | Verdict |
|---|---|
| E1 depend | ❌ We'd inherit advisory legality, unfactored decisions, auto-pay defaults, and 200–2,000× throughput gaps, on a nightly toolchain and a 1.7M-line API surface moving at ~7 commits a day. |
| E2 fork | ❌ The same gaps, plus owning 1.7M lines we didn't design. Fixing legality by construction and throughput would mean rewriting the core anyway. |
| **E3 borrow selectively** | ✅ **Recommended.** Build the engine core clean-room per ADR-0004/0005 and our NFRs. Borrow where phase.rs is strong: its Oracle parser and IR as the leading input to [[ADR-0007 Card Definition Approach]], its coverage report as a differential oracle, and its determinization and visibility designs as references. |
| E4 ignore | ❌ It would throw away the best available evidence on card parsing. |

Concrete E3 actions:
1. **M3 spike:** include "phase.rs parser as a front-end" as a contender. One option is an offline tool that depends on `phase-engine`'s parser to emit candidate definitions into our IR. Another is porting the grammar. Measure both against the 205-card pool ([[Card Definition Spike Plan]]).
2. **Differential testing (Q24):** compare our per-card coverage and behaviour with phase.rs's on the cEDH pool and flag disagreements for review.
3. **Design references:** `determinize.rs` (the CR 400.2 / 401.2 / 701.20 pinning rules), `visibility.rs` (redaction), and its CR-annotated `WaitingFor` variants as a checklist of decision kinds we must cover.
4. **Re-evaluate at M6.** If phase.rs later fixes legality-by-construction and throughput, revisit E1 for the card layer only.

## Open follow-ups
- A lighter enumeration path inside phase.rs (if one exists) would shrink, not close, the throughput gap. It's worth a note to upstream.
- License hygiene for any borrowed code: keep MIT/Apache headers and a NOTICE entry.

## Sources
- phase.rs repository, commit `7b1504a` (2026-10-20): https://github.com/phase-rs/phase. Files read: `crates/engine/src/types/game_state.rs` (`WaitingFor`, `CastPaymentMode`), `crates/engine/src/types/actions.rs`, `crates/engine/src/ai_support/mod.rs` (`legal_actions`), `crates/engine/src/game/engine.rs` (`apply*`), `crates/engine/src/game/visibility.rs`, `crates/engine/src/types/replay.rs`, `crates/phase-ai/src/determinize.rs`, `crates/phase-ai/src/config.rs`, `crates/engine/src/game/deck_loading.rs`
- Published coverage and card data (generated 2026-10-19, MTGJSON 5.3.0+20261019): https://data.phase-rs.dev/coverage-summary.json, https://data.phase-rs.dev/coverage-data.json, https://data.phase-rs.dev/card-data-meta.json
- Bench source: an out-of-tree crate written for this evaluation (random policy over `legal_actions` plus `apply_as_current`). Not committed, because it depends on a 100 MB card-data download.
- Our NFR targets: [[Non-Functional Requirements]]. Design principles: [[Design Principles]].
