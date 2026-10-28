---
type: hub
status: living
tags: [hub, moc]
updated: 2026-10-27
---

# Kiln: Home

> [!abstract] One sentence
> A **rules-exact Magic: The Gathering engine in Rust** that powers a **simulation lab your AI can drive**. Ask "how does deck X do in a cEDH pod against Y, Z and C?" and get thousands of seeded games and statistically honest answers.

**Phase:** M0, groundwork (design docs only). See [[Roadmap]] and [[M0 Groundwork]].
**Comprehensive Rules baseline:** effective **2026-06-19** (see [[CR Structure]]).

## The headline user story
"Evaluate my Kinnan deck in a cEDH pod against Tymna/Kraum, Najeela and Thrasios/Tymna over 5,000 games, and tell me the win rates, how fast it wins, and which cards are pulling their weight." That request flows through these pieces:
[[User Stories]] → [[Experiment Spec]] → [[MCP Server Tool Catalog]] → [[Policy Composition]] → [[Batch Runner & Parallelism]] → [[Metrics Catalog]] → [[Output Formats & Report Schema]]

## Map of content
| Area | Start here | What's inside |
|---|---|---|
| Vision | [[Vision & Goals]] | [[Scope & Non-Goals]], [[Design Principles]], [[Glossary]], [[User Stories]] |
| Requirements | [[Functional Requirements]] | [[Non-Functional Requirements]], [[Formats & Game Modes]] |
| Architecture | [[Architecture Overview]] | [[Crate Layout]], [[Game State Model]], [[Decision & Action Model]], [[Event System]], [[Hidden Information & Player Views]], [[Determinism & Replay]], [[Performance & Cheap Cloning]] |
| Rules subsystems | [[Subsystems Index]] | One spec per CR subsystem (stack, layers, combat, Commander, …) |
| Rules map | [[CR to Subsystem Map]] | [[CR Structure]], [[Tricky Interactions Catalog]], [[CR Update Process]], `Rules Coverage.base` |
| Card definitions | [[Card Definitions Problem Statement]] | Options, [[Oracle Text Corpus Analysis]], [[Evaluation Criteria]], [[Card Definition Spike Plan]] |
| Card data | [[Card Data Sources]] | [[Ingestion Pipeline]], [[Card Data Model]], [[Legal & Licensing]] |
| Interfaces | [[AI-Agent Integration Research]] | [[Library API]], [[CLI & REPL]], [[MCP Server Tool Catalog]], [[Python Package]], [[WASM Build]], [[Judge Tool UX]], [[Agent-Facing Docs Plan]], [[AGENT_GUIDE]] |
| Simulation | [[Simulation Harness]] | [[Experiment Spec]], [[Deck Import]], [[Batch Runner & Parallelism]], [[Metrics Catalog]], [[Output Formats & Report Schema]], [[Reproducibility]], [[Validity & Caveats]] |
| Players & policies | [[Policy Interface]] | [[A - Baseline Policies]], [[B - Playbooks]], [[C - LLM Live Decisions]], [[D - Search AI]], [[Policy Composition]], [[Measuring Play Quality]] |
| Testing | [[Testing Strategy]] | [[CR-Cited Scenario Tests]], [[Scenario File Format]], [[Golden Replays]], [[Invariants & Property Tests]], [[Fuzzing via Random Play]] |
| Roadmap | [[Roadmap]] | Milestones M0–M12, [[Curated cEDH Core Pool]] |
| Decisions | [[ADR Index]] | ADR-0001 … ADR-0013 |
| Research | [[Research Index]] | Prior art, search AI, LLM play, statistics, Rust ecosystem |
| Open questions | [[12 Open Questions]] | Everything not yet decided |

## Decisions so far (summary)
- **Accepted:**
  - Rust ([[ADR-0001 Rust as Engine Language]])
  - MIT/Apache, with card data fetched rather than bundled ([[ADR-0002 Licensing and Card Data Distribution]])
  - docs-as-vault ([[ADR-0003 Docs as Obsidian Vault in Repo]])
  - all four policy kinds ([[ADR-0006 Pluggable Decision-Makers]])
  - Commander-first ([[ADR-0010 Commander-First N-Player Core]])
  - project name Kiln, crate `kiln-mtg` ([[ADR-0013 Project Name]])
- **Proposed (recommended):**
  - pure decision state machine ([[ADR-0004 Engine as Decision State Machine]])
  - determinism ([[ADR-0005 Determinism and Replay]])
  - interfaces ([[ADR-0008 AI-Agent and External Interfaces]])
  - data source ([[ADR-0009 Card Data Source of Truth]])
  - experiment schema ([[ADR-0011 Experiment Spec and Report Schema]])
- **Open:**
  - card definition approach ([[ADR-0007 Card Definition Approach]])
  - relationship to phase.rs ([[ADR-0012 Relationship to phase.rs]])

## Vault conventions
- Frontmatter on every note: `type`, `status` (draft | proposed | accepted | living | superseded), `tags`, `updated`. Subsystem notes add `cr` and `impl_status`.
- CR citations look like `CR 603.3b`. Verify them against `_external/cr-lf.txt`, which is gitignored and holds the local copy.
- New notes start from `_templates/`.
- Research notes end with `## Sources`.
