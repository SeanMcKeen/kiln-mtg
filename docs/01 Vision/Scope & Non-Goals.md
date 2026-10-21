---
type: vision
status: accepted
tags: [vision, scope]
updated: 2026-10-20
---

# Scope & Non-Goals

## In scope
- **Rules engine:**
  - the complete Comprehensive Rules for the formats below
  - N-player from day one
  - hidden information
  - determinism
- **Card support:** eventually the full card pool (about 32k Commander-legal Oracle cards as of 2026-10). The first target is the [[Curated cEDH Core Pool]].
- **Formats:**
  - Commander/cEDH, the **primary** target
  - 1v1 constructed
  - Two-Headed Giant and team variants
  - Limited (sealed/draft pools)

  See [[Formats & Game Modes]].
- **Simulation lab:**
  - experiment specs
  - batch runner
  - metrics with toggle flags
  - reports
  - replays
- **Decision-makers (all four, pluggable):**
  - baseline policies
  - AI-authored playbooks
  - live LLM decisions
  - generic search AI

  See [[ADR-0006 Pluggable Decision-Makers]].
- **Interfaces:**
  - Rust library
  - CLI/REPL
  - MCP server
  - JSON protocol
  - Python bindings
  - WASM, later

  See [[ADR-0008 AI-Agent and External Interfaces]].
- **Rules judge tool:** scenario setup, stepping through actions, CR-cited explanations.
- **Card data ingestion** from Scryfall and/or MTGJSON ([[ADR-0009 Card Data Source of Truth]]).

## Out of scope (non-goals)
| Non-goal | Why | Note |
|---|---|---|
| Playable game client / GUI | Not the product. Others can build one on the library. | — |
| Hosting online multiplayer for humans | Same reason as above. The JSON protocol makes it possible for others. | — |
| Bundling card data, images, or CR text | Fan Content Policy and licensing. Everything is fetched. | [[Legal & Licensing]] |
| Deck building / optimization AI | It may come later as a consumer of the sim lab, not part of the core. | [[12 Open Questions]] |
| Un-sets, Acorn cards, silver-border | They don't follow the CR. Possibly later behind a flag. | — |
| Casual variants beyond Commander (Planechase, Archenemy, Vanguard, Conspiracy Draft) | Low value for the primary use case. The architecture shouldn't preclude them (CR 901–905). | [[Formats & Game Modes]] |
| Ante (CR 407) | Banned everywhere, so it has no use for simulation. | — |
| Arena-only digital mechanics (Alchemy "seek", perpetual, etc.) | They aren't in the paper CR. Maybe later. | [[12 Open Questions]] |

## Scope boundaries to watch
> [!warning] Scope creep risks
> - **Search AI quality** is unbounded research. [[D - Search AI]] has an explicit "good enough" bar ([[Measuring Play Quality]]).
> - **Full card pool.** 89% of distinct Oracle ability lines are unique ([[Oracle Text Corpus Analysis]]), so the long tail is real work. Coverage is tracked, and experiments are gated on it.
> - **LLM live play** is slow and expensive. It's scoped to spot checks and validation, not bulk runs.

## Related
[[Vision & Goals]] · [[Formats & Game Modes]] · [[Roadmap]]
