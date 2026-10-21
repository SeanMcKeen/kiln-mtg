---
type: research
status: draft
tags: [research, prior-art]
updated: 2026-10-20
---

# Prior Art: XMage

**Repo:** https://github.com/magefree/mage. **License: MIT** (verified 2026-10-20). About 2.4k stars, very active.

## Model
- Java, **one class per card**, built from a large shared library of Ability, Effect, Cost and Condition classes. The README claims "full rules enforcement for over 19,200 unique cards" (per a mirrored README).
- Ships deterministic, non-LLM AI players.
- **MageBench** used XMage plus MCP tools to let LLMs play ([[LLMs Playing MTG]]).

## Lessons
- Code-per-card scales with a large contributor base, but the effect library is what really carries the load ([[Option - Code per Card]]).
- The class names and internal layer handling (`ContinuousEffect`, `Layer`, `SubLayer`) weren't confirmed in this research pass. Read the source if needed.
- The Go port **mage-go** (Go, early Magic, 2-player) re-applies continuous effects from scratch in layer order each cycle. That's a simple baseline strategy for [[Continuous Effects & Layers]].

## Sources
- GitHub LICENSE: https://raw.githubusercontent.com/magefree/mage/master/LICENSE.txt
- Mirror README (card counts): https://github.com/ryegleason/mage, https://github.com/Grumme2/mage
- mage-go docs: https://pkg.go.dev/github.com/benprew/mage-go
