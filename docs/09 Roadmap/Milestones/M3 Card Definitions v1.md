---
type: milestone
status: planned
milestone: M3
tags: [milestone]
updated: 2026-10-20
---

# M3: Card Definitions v1

**Goal:** decide how cards are defined, and get the first 100 non-trivial cards verified.

## Checklist
- [ ] Run the [[Card Definition Spike Plan]] → accept [[ADR-0007 Card Definition Approach]]
- [ ] IR v1: effects, filters, costs, triggers, statics, replacements (the shape at least; behaviour arrives in M4/M5)
- [ ] Chosen front-end(s) implemented (parser, DSL loader, escape registry)
- [ ] IR → text renderer (lint against Oracle)
- [ ] Targeting & legality ([[Targeting & Legality]])
- [ ] Evergreen keywords ([[Keywords & Keyword Actions]]): flying, reach, trample, first and double strike, deathtouch, lifelink, vigilance, haste, menace, defender, flash, hexproof, shroud, indestructible, ward, protection
- [ ] Counters ([[Counters]])
- [ ] Greedy baseline policy ([[A - Baseline Policies]])
- [ ] 100 cards verified (removal, counterspells, card draw, tutors, mana rocks)

## Exit criteria
ADR-0007 accepted. 100 cards `verified`. A random-policy fuzz over those cards is clean.
