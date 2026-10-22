---
type: milestone
status: planned
milestone: M2
tags: [milestone]
updated: 2026-10-22
---

# M2: Core Game Loop

**Goal:** an N-player engine that plays complete games with lands, mana, vanilla and French-vanilla creatures, simple spells (hand-coded IR), combat, SBAs, deterministic replays, and a random policy.

## Checklist
- [ ] `GameState`, object arena, zones ([[Game State Model]], [[Zones & Objects]])
- [ ] Decision model v1: `PendingDecision`/`Choice`, continuations ([[Decision & Action Model]]) → accept ADR-0004
- [ ] Turn structure and turn-based actions ([[Turn Structure]])
- [ ] Priority and the stack, resolution ([[Priority & the Stack]], [[Resolution]])
- [ ] Mana pools, mana abilities, cost payment v1 with plan enumeration ([[Costs & Mana]])
- [ ] Casting procedure skeleton per CR 601.2 ([[Casting & Activating]])
- [ ] Combat with factored declarations ([[Combat]])
- [ ] Core SBAs (704.5a–j) ([[State-Based Actions]])
- [ ] Mulligans (London, multiplayer free mulligan)
- [ ] Seeded RNG, replay log, determinism tests → accept ADR-0005 ([[Determinism & Replay]])
- [ ] PlayerView v1 with knowledge tracking basics ([[Hidden Information & Player Views]])
- [ ] Event system v1 ([[Event System]])
- [ ] `kiln-policy`: the Policy trait, random, passive, scripted
- [ ] Sim skeleton: run N random games in parallel. Invariant checks ([[Invariants & Property Tests]])
- [ ] Benchmarks: clone cost, games/sec → decide the persistent vs. arena state strategy ([[Performance & Cheap Cloning]])
- [ ] Scenario file loader v1 ([[Scenario File Format]])

## Exit criteria
1M random 4-player games with vanilla decks: no panics, no invariant violations, 100% replay determinism. Benchmarks recorded.
