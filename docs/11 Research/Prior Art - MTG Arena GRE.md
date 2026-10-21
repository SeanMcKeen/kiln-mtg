---
type: research
status: draft
tags: [research, prior-art, parser]
updated: 2026-10-20
---

# Prior Art: MTG Arena's GRE

From Wizards' developer posts. Proprietary, so we take ideas only.

## Architecture
- **GRE (Game Rules Engine):** C++ plus **CLIPS** (a LISP-like rules language). It handles core rules (priority, lethal damage, phases, casting steps) and **doesn't know what individual cards do**.
- **Card behaviour = CLIPS rules.** The **GRP** (a Python program) "takes raw English rules text of Magic cards and converts them into one or more CLIPS rules", which handles **"80% or so of newly written Magic cards"** automatically. The rest need parser updates.
- **The "whiteboard and naps" model:**
  1. The GRE writes a to-do list (e.g. "destroy these creatures") or a list of available actions, then "naps".
  2. CLIPS rules edit the list. Indestructible removes a destroy. Unearth turns a destroy into an exile. Shield counters turn a destroy into removing a counter.
  3. The GRE wakes and executes what's left.

  This is effectively a **proposed-event / replacement pipeline** plus **action-list filtering** (Meddling Mage removes the Lightning Bolt cast options).
- Living Breakthrough needed three new rules and edge-case fixes (X spells; storing mana value instead of the spell ID because IDs persist through resolution).

## Lessons for us
1. **An engine that doesn't know cards** is the right separation ([[Architecture Overview]]).
2. **Proposed events edited by effects** ↔ our [[Event System]] and [[Replacement & Prevention]] design.
3. **Legal-action lists edited by restrictions** ↔ our legality-by-construction menus ([[Decision & Action Model]]).
4. **A parser can carry ~80% of cards.** Supports [[Option - Hybrid IR]].

## Sources
- "On Whiteboards, Naps, and Living Breakthrough": https://magic.wizards.com/en/news/mtg-arena/on-whiteboards-naps-and-living-breakthrough
- Wikipedia: https://en.wikipedia.org/wiki/Magic:_The_Gathering_Arena
