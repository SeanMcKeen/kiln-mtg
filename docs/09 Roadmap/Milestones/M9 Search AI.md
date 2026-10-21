---
type: milestone
status: planned
milestone: M9
tags: [milestone, ai]
updated: 2026-10-20
---

# M9: Search AI

## Checklist
- [ ] Determinization service in the engine ([[Policy Interface]])
- [ ] SO-ISMCTS with iteration and time budgets, max^n multiplayer backups ([[D - Search AI]])
- [ ] Playouts with a greedy or playbook rollout policy. Progressive widening
- [ ] Evaluation function for truncated playouts
- [ ] Playbook-guided search (prior and rollout)
- [ ] Puzzle suite and policy ladder ([[Measuring Play Quality]])

## Exit criteria
Meets the v1 bar in [[Measuring Play Quality]]. The US-1 run with guided search completes 5,000 games in under 1 hour on 8 cores.
