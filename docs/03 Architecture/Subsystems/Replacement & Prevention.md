---
type: subsystem
status: draft
cr: ["614", "615", "616"]
impl_status: not-started
milestone: M4
tags: [subsystem, replacement, prevention]
updated: 2026-10-20
---

# Replacement & Prevention

## Purpose
This subsystem modifies events *before* they happen ("instead", "skip", "enters with", "as ~ enters", "prevent").

## CR sections
614 (replacement effects), 615 (prevention effects), 616 (interaction of replacement and/or prevention effects).

## Responsibilities
- Run every **proposed event** ([[Event System]]) through the registry of applicable replacement and prevention effects.
- Apply the **CR 616.1 procedure** when several apply. The affected object's controller (or its owner, if it has no controller) or the affected player chooses, with the forced ordering:
  1. self-replacement effects (616.1a)
  2. control-changing ETB (616.1b)
  3. copy-on-ETB (616.1c)
  4. back-face-up ETB (616.1d)
  5. then any (616.1e)

  Repeat until none apply (616.1f).
- Each replacement applies **at most once** per event (CR 614.5). Newly applicable effects are considered after modification (CR 616.2).
- "Enters with" and "as enters" replacements (CR 614.1c, 614.12), including those from the entering permanent itself.
- Skip effects (Necropotence skips the draw step).
- Prevention shields ("prevent the next 3 damage") consume their shield.

## Decisions raised
- **`ChooseReplacement`**, which effect to apply next (CR 616.1). It is skipped when only one applies.
- "May" replacements: commander "may put it into the command zone instead" (CR 903.9b), Mox Diamond's "discard a land or put it into the graveyard" (an ETB replacement with a choice).

## Edge cases (cEDH)
- **Notion Thief:** an opponent's draws, except the first in each draw step, are replaced with you drawing. Interacts with wheels and Bowmasters.
- **Rest in Peace / Leyline of the Void:** graveyard replacement vs. a commander's command-zone replacement, an ordering choice for the commander's owner.
- **Containment Priest / Grafdigger's Cage:** "enters from anywhere other than a graveyard or library" restrictions (Cage is a "can't").
- **Teferi's Protection:** life total can't change, protection from everything, phase out. Mixed replacement and continuous effects.
- **Worldgorger Dragon + Animate Dead:** zone-change chains feeding the loop detector ([[Loops & Shortcuts]]).

## Related
[[Event System]] · [[Zones & Objects]] · [[Commander]]
