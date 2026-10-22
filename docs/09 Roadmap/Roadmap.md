---
type: roadmap
status: living
tags: [roadmap]
updated: 2026-10-20
---

# Roadmap

The roadmap is **Commander-first** ([[ADR-0010 Commander-First N-Player Core]]). The headline use case (US-1) becomes credible at **M9**: rules and cards (M6), the harness (M7), agent access (M8), and competent play (M9).

```
M0 Groundwork ─► M1 Foundations & Data ─► M2 Core Game Loop ─► M3 Card Definitions v1
                                                │                     │
                                                ▼                     ▼
                                      (sim skeleton + random)   M4 Triggers & Replacement ─► M5 Layers & Copy
                                                                                               │
                                                     M6 Commander & cEDH Core Pool ◄───────────┘
                                                              │
                                     M7 Simulation Lab v1 ◄───┘
                                              │
                         M8 Agent Integration & Playbooks ─► M9 Search AI ─► M10 LLM Bridge & Judge
                                                                                 │
                                    M11 Full Card Pool (continuous from M6) ◄────┘
                                    M12 More Formats & Bindings
```

| # | Milestone | Goal (one line) | Key exit criterion |
|---|---|---|---|
| M0 | [[M0 Groundwork]] | Scope, research, vault, decisions | This vault. ADRs 1–3, 6 and 10 accepted |
| M1 | [[M1 Foundations & Data]] | Toolchain, workspace, core types, card DB, deck import, CR parser, phase.rs evaluation | `mtg deck check` validates a real Commander list with coverage output |
| M2 | [[M2 Core Game Loop]] | N-player turns, priority, stack, mana, lands, vanilla creatures, basic combat, SBAs, decisions, replay | 1M random 4-player vanilla games, no invariant violations, deterministic replays |
| M3 | [[M3 Card Definitions v1]] | Card-definition spike → ADR-0007, IR v1, evergreen keywords | 100 cards verified, including targeted spells, activated abilities, counterspells |
| M4 | [[M4 Triggers & Replacement]] | Triggers, replacements, full SBAs, loops and shortcuts | Tricky catalog T30–T37 pass (T12 moved to M5); **in review** |
| M5 | [[M5 Layers & Copy]] | Layers, dependencies, copy effects, control change | T50–T55 and T07–T08 pass (T12 deferred, Q26/Q27); **in review** |
| M6 | [[M6 Commander & cEDH Core Pool]] | Commander rules, multiplayer, the ~200-card cEDH pool verified | Four real-ish cEDH decks play full games with random and greedy policies |
| M7 | [[M7 Simulation Lab v1]] | Specs, runner, metrics with flags, reports, CLI JSON | US-2 and US-4 work. US-1 runs (with baseline-quality caveats) |
| M8 | [[M8 Agent Integration & Playbooks]] | MCP server, agent docs, playbook policy | An agent completes US-1 end to end via MCP |
| M9 | [[M9 Search AI]] | ISMCTS player, playbook-guided search, play-quality suite | Meets the [[Measuring Play Quality]] v1 bar |
| M10 | [[M10 LLM Bridge & Judge]] | Live LLM policy, judge REPL with explanations | US-5 and US-6 work |
| M11 | [[M11 Full Card Pool]] | Scale to all Commander-legal cards, cEDH-legal first | ≥ 95% of cEDH-relevant cards verified, coverage dashboard |
| M12 | [[M12 More Formats & Bindings]] | 1v1, 2HG and teams, Limited, Python, WASM | Each format has its own scenario suite. Wheels and npm published |

**Ongoing from M1:** the [[CR Update Process]], card DB refreshes, and validation against [[cEDH Ground-Truth Data]].

## GitHub mirroring (later)
Each milestone becomes a GitHub milestone. Each `- [ ]` item becomes an issue labelled `area:<subsystem>`, `kind:<feature|research|test|docs>`, and `cr:<section>` where applicable.
