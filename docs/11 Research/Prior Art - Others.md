---
type: research
status: draft
tags: [research, prior-art]
updated: 2026-10-20
---

# Prior Art: Others

| Project | Notes |
|---|---|
| **mage-go** | A Go engine inspired by XMage. Early Magic, 2-player only. Layered continuous effects re-applied each cycle. https://pkg.go.dev/github.com/benprew/mage-go |
| **Magarena** | A Java/Groovy single-player engine with AI. Historically GPL. Not researched in depth this pass. |
| **Cockatrice** | A tabletop client with no rules enforcement. Not relevant to rules, but its deck formats are widely used. |
| **Goldfish/bracket calculators** (e.g. scrollvault Commander Bracket Calculator) | Monte Carlo goldfish of a 99-card list (mulligans, land drops, ramp, draw, tutors) with a fixed seed. **No opponents and no full rules.** This shows demand for our US-2 and US-4. https://scrollvault.net/tools/commander-bracket/ |
| **A private "port XMage to Rust for RL" trace** | Anecdotal evidence that others want a fast Rust engine for RL training. |

## Takeaway
No existing open tool combines **full rules**, **N-player cEDH**, **competent pluggable AI**, and **agent-driven experiments with statistics**. That's the niche.

## Sources
- mage-go: https://pkg.go.dev/github.com/benprew/mage-go
- scrollvault Commander Bracket Calculator: https://scrollvault.net/tools/commander-bracket/
- Rust-port-for-RL trace (anecdotal): https://traces.com/api/v1/traces/155988fc-6593-4b7f-8230-5d054f3e30af/download
