---
type: research
status: draft
tags: [research, prior-art]
updated: 2026-10-20
---

# Prior Art: Forge

**Repo:** https://github.com/Card-Forge/forge. **License: GPL-3.0**, which is incompatible with our license, so we don't copy code or scripts. About 2.8k stars, active (pushed 2026-10-20).

## Card scripting model
Each card is a text script with keys:
- `Name`, `ManaCost`, `Types`, `PT`
- `K:` keywords (`Kicker:{cost}`)
- **`A:` abilities** (e.g. `A:SP$ DealDamage | ValidTgts$ … | NumDmg$ … | SubAbility$ …`)
- **`T:` triggers**, **`S:` statics**, **`R:` replacements**
- **`SVar`** named variables and sub-abilities
- `Oracle:` text, `DeckHints`, `DeckNeeds`, `DeckHas`

The scripting conventions:
- `SP$` marks spell abilities, with defaults omitted (`SP$ Draw` rather than `SP$ Draw | Defined$ You | NumCards$ 1`)
- flags like `TargetUnique$ True` handle "another target"

## AI hints embedded in card scripts
`AI:RemoveDeck` (All/Random/NonCommander), and SVars like `AIEvaluationModifier`, `AIPriorityModifier`, `SacMe`, `NeedsToPlayVar`, `PlayMain1`.

**Lesson:** a heuristic AI needs per-card hints, and Forge co-locates them with the card definition. Our playbooks ([[B - Playbooks]]) put this knowledge per *deck* instead, authored by AI agents, and keep it out of the rules data.

## Lessons
- A declarative DSL plus code is proven at about 30k-card scale with a big community ([[Option - DSL with Escape Hatch]]).
- The scripts are terse and pipe-delimited, which is hard to read and validate. If we use a DSL, prefer a structured format (YAML/RON) with a schema.

## Sources
- Card scripting API wiki: https://github.com/Card-Forge/forge/wiki/Card-scripting-API
- Card scripting wiki: https://github.com/Card-Forge/forge/wiki/cardscripting
- Forum example of `A:SP$ DealDamage`: https://www.slightlymagic.net/forum/viewtopic.php?p=69490
- GitHub API (license and activity), queried 2026-10-20
