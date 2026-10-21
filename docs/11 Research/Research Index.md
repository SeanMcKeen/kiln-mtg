---
type: index
status: living
tags: [research, index]
updated: 2026-10-20
---

# Research Index

## Prior art
- [[Prior Art - phase.rs]]: the closest analog (Rust, MIT/Apache, Oracle parser). ⭐ Highest leverage. M1 hands-on evaluation: [[phase.rs Evaluation]].
- [[Prior Art - Forge]]: Java, GPL-3.0, card script DSL, AI hints.
- [[Prior Art - XMage]]: Java, MIT, code per card. Used by MageBench.
- [[Prior Art - MTG Arena GRE]]: C++/CLIPS, a parser for ~80% of new cards, the whiteboard model.
- [[Prior Art - Others]]: mage-go, Magarena, goldfish calculators.

## Theory & AI
- [[MTG Complexity & Loops]]: MTG is Turing complete, so caps and shortcuts are necessary.
- [[MCTS & ISMCTS for MTG]]: ensemble determinization (Cowling et al. 2012), ISMCTS.
- [[LLMs Playing MTG]]: MageBench, MTG Bench, UrzaGPT.

## Data & statistics
- [[Card Data Sources]] (in 06): Scryfall vs. MTGJSON.
- [[Oracle Text Corpus Analysis]] (in 05): 89% of distinct ability lines are unique.
- [[Statistics for Multiplayer Simulation]]: Wilson CIs, seat effects, paired A/B.
- [[cEDH Ground-Truth Data]]: EDHTop16 GraphQL API.

## Engineering
- [[Rust Ecosystem]]: candidate crates.
- [[AI-Agent Integration Research]] (in 07): MCP, CLI, Python. Tool-design guidance.

## Research backlog
- [x] Read phase.rs source: parser, decision model, AI (M1) → [[phase.rs Evaluation]]
- [ ] Read the full Cowling et al. 2012 papers (M9)
- [ ] Introspect the EDHTop16 GraphQL schema and check usage terms (M11)
- [ ] Topdeck.gg data availability
- [ ] Verify the ⚠ entries in the [[Tricky Interactions Catalog]] against Scryfall rulings
- [ ] Magarena architecture (low priority)
- [ ] LLM authorability: Rhai vs. Lua vs. YAML playbooks (M8)
