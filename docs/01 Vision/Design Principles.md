---
type: vision
status: accepted
tags: [vision, principles, architecture]
updated: 2026-10-20
---

# Design Principles

These are the tie-breakers. When two designs compete, the one that better satisfies these, in this order, wins.

## 1. Legality by construction
The engine is a **state machine that only ever offers legal options**. At every point it returns a `PendingDecision` that holds the complete legal option set for one player, and it accepts a `Choice` that must be one of those options. Clients never get a "try this and see if it errors" API.
→ [[Decision & Action Model]], [[ADR-0004 Engine as Decision State Machine]]

## 2. Every CR-sanctioned choice is a first-class decision
If the CR lets a player choose, the engine asks. That covers:
- trigger order (CR 603.3b)
- replacement/prevention order (CR 616.1)
- division of combat damage (CR 510.1c)
- mana payment and mana-ability activation
- targets, modes, X, "may"
- order of cards put on the bottom or top
- legend-rule choice (CR 704.5j)
- shortcut loop counts (CR 732.2a)
- and more

Convenience automation (e.g. "auto-order my triggers") is a **policy** feature. It never lives in the engine.

## 3. Determinism and replayability
Given the same card database version, rules version, decklists, policy configuration and seed, a game is **bit-identical**. Every game can be reduced to `(initial setup, seed, choice log)` and replayed.
→ [[Determinism & Replay]], [[ADR-0005 Determinism and Replay]]

## 4. CR traceability
Every rule implementation cites its CR number in code, and every subsystem spec lists the CR sections it owns. Tests are named and tagged by rule. The CR → subsystem map and coverage tracker show what's done.
→ [[CR to Subsystem Map]], [[CR-Cited Scenario Tests]]

## 5. Honest information boundaries
Policies, including search AI and LLMs, only see a **redacted player view**. Cheating has to be impossible by construction, because otherwise simulation results are invalid. The search AI "determinizes" hidden information by sampling. It never peeks.
→ [[Hidden Information & Player Views]]

## 6. Built for simulation throughput
Throughput affects the design from the start. That means:
- cheap state cloning (search AI)
- no allocation-heavy hot paths
- no global mutable state
- `Send + Sync` games, for parallel batch runs
- cheap "is anything listening?" checks for metrics

→ [[Performance & Cheap Cloning]], [[Non-Functional Requirements]]

## 7. Separation of concerns
| Layer | Owns | Knows nothing about |
|---|---|---|
| Rules core | game state, CR procedures, decisions | specific cards, policies, I/O |
| Card definitions | what each card does, compiled to an IR | how rules procedures work internally |
| Card data | ingesting Oracle data | rules |
| Policies | choosing among legal options | engine internals (they use the public API only) |
| Simulation | experiments, metrics, reports | how policies decide |
| Interfaces | MCP, CLI, Python, WASM, JSON | anything beyond the public API |

→ [[Crate Layout]]

## 8. Agent-first ergonomics
Every capability is reachable by an AI agent through a documented, token-efficient interface. That means:
- concise and detailed response modes
- names instead of opaque IDs
- actionable errors
- long-running jobs with progress

→ [[AI-Agent Integration Research]], [[MCP Server Tool Catalog]]

## 9. Fail loudly on unsupported cards
An unimplemented or unverified card must **never** silently behave like a blank card in an experiment. Coverage status is data, and experiments check it before running.
→ [[Validity & Caveats]], [[Card Data Model]]

## 10. Measure, don't assume
Performance, play quality, and statistical confidence are all measured and reported, never asserted.
→ [[Measuring Play Quality]], [[Statistics for Multiplayer Simulation]]
