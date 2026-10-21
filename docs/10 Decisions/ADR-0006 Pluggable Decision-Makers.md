---
type: adr
status: accepted
decided: 2026-10-20
supersedes: "initial scoping answer 'AI opponents out of scope'"
tags: [adr, policies]
updated: 2026-10-20
---

# ADR-0006: Pluggable Decision-Makers (all four kinds)

## Context
The user initially put AI opponents out of scope, then clarified that **the main use case is AI-driven deck simulation** ("evaluate X in a cEDH pod vs Y, Z, C over thousands of games"). Meaningful results need competent play. After the four options were explained, the user chose **all of them, used when the situation calls for it, or available as options.**

## Decision
Four policy kinds live in scope, in separate crates from the rules core, behind one `Policy` interface:
- **A. Baselines** (random, greedy, passive, scripted): [[A - Baseline Policies]]
- **B. AI-authored playbooks** (sandboxed scripts): [[B - Playbooks]]
- **C. Live LLM decisions** (spot checks): [[C - LLM Live Decisions]]
- **D. Generic search AI** (ISMCTS): [[D - Search AI]]

They are composable per seat, per decision kind, and per condition ([[Policy Composition]]).

## Consequences
- The engine must expose a determinization service and redacted views ([[Hidden Information & Player Views]]).
- Play quality becomes a measured property ([[Measuring Play Quality]]).
- Scope grows: M8–M10 in the [[Roadmap]].
