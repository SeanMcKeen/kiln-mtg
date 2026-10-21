---
type: reference
status: living
tags: [glossary]
updated: 2026-10-20
---

# Glossary

Project-specific terms. For MTG terms, the CR glossary (at the end of `_external/cr-lf.txt`) is authoritative.

| Term | Meaning in this project |
|---|---|
| **APNAP** | Active Player, Non-Active Player order (CR 101.4). Used whenever several players act or choose simultaneously. |
| **Choice** | A client's answer to a `PendingDecision`. It must be one of the offered options. See [[Decision & Action Model]]. |
| **Coverage status** | Per-card implementation state: `unsupported` → `parsed` → `verified`. Experiments require `verified`. See [[Card Data Model]]. |
| **CR** | The Magic: The Gathering Comprehensive Rules. The baseline is effective 2026-06-19. |
| **Decision factoring** | Splitting a combinatorially large choice (e.g. declaring 20 attackers across 3 defenders) into a sequence of small decisions. See [[Decision & Action Model]]. |
| **Determinization** | Sampling a concrete guess at hidden information (opponents' hands, library order) consistent with what a player knows, so that perfect-information search can run. See [[D - Search AI]]. |
| **Experiment** | A declarative spec of decks, seats, policies, game count, seed and metric flags. See [[Experiment Spec]]. |
| **Goldfish** | Playing a deck against no opposition to measure its speed. A supported experiment mode. |
| **Golden replay** | A stored `(setup, seed, choice log, expected final state hash)` used as a regression test. See [[Golden Replays]]. |
| **IR (card IR)** | The typed intermediate representation every card definition compiles to, whatever authoring front-end is used. See [[ADR-0007 Card Definition Approach]]. |
| **ISMCTS** | Information Set Monte Carlo Tree Search. See [[MCTS & ISMCTS for MTG]]. |
| **Oracle ID** | Scryfall/WotC identifier for a card's rules identity, shared across printings. It is the primary card key. See [[ADR-0009 Card Data Source of Truth]]. |
| **PendingDecision** | What the engine returns when it needs input: who decides, what kind of decision, and the full legal option set plus context. |
| **Playbook** | A sandboxed, AI-authored, per-deck script that guides play (mulligans, lines, priorities). See [[B - Playbooks]]. |
| **Player view** | The redacted state a specific player is allowed to see. The only thing policies receive. See [[Hidden Information & Player Views]]. |
| **Policy** | Anything that answers decisions for a seat: baseline, playbook, search, LLM, human, or a composition of these. See [[Policy Interface]]. |
| **Pod** | A Commander table, normally 4 players. |
| **Run** | One execution of an experiment, producing a report and artifacts. |
| **Seat** | A position in turn order. Seat 1 goes first unless the spec randomizes or rotates seating. |
| **Shortcut** | A CR 732 sequence of choices proposed and accepted as a block, e.g. "repeat this loop 1,000 times". See [[Loops & Shortcuts]]. |
| **SBA** | State-based action (CR 704). |
| **Timestamp** | The ordering key for continuous effects (CR 613.7). |
