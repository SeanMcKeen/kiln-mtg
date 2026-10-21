---
type: milestone
status: planned
milestone: M7
tags: [milestone, simulation]
updated: 2026-10-20
---

# M7: Simulation Lab v1

## Checklist
- [ ] `mtg-protocol` with JSON Schemas: spec, report, replay ([[ADR-0011 Experiment Spec and Report Schema]])
- [ ] Experiment spec parsing and validation ([[Experiment Spec]])
- [ ] Batch runner: rayon, deterministic seeds, checkpointing, progress, cancellation ([[Batch Runner & Parallelism]])
- [ ] Metrics with flags: win_rates, game_shape, mulligans, per_card, interaction, mana, timing, decision_stats, replays, raw_events ([[Metrics Catalog]])
- [ ] Statistics: Wilson and bootstrap CIs, seat analysis, BH correction ([[Statistics for Multiplayer Simulation]])
- [ ] Reports: concise and detailed JSON, markdown, Parquet tables ([[Output Formats & Report Schema]])
- [ ] Modes: pod, goldfish, ab, opening_hands
- [ ] CLI: `mtg sim run|status|report|compare`, `mtg game replay|inspect` ([[CLI & REPL]])
- [ ] Validity warnings ([[Validity & Caveats]])

## Exit criteria
US-2 and US-4 work end to end. US-1 runs and produces a report with correct CIs and validity warnings. 5,000 greedy games finish in under 1 minute on 8 cores.
