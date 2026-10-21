---
type: subsystem
status: draft
cr: ["108", "109", "110", "111", "112", "113", "114", "400", "401", "402", "403", "404", "405", "406", "408"]
impl_status: not-started
milestone: M1
tags: [subsystem, zones, objects]
updated: 2026-10-20
---

# Zones & Objects

## Purpose
This subsystem owns *what exists and where*: cards, tokens, copies, emblems, and abilities on the stack, plus the zones that hold them and the zone-change primitive every effect uses.

## CR sections
108 (Cards), 109 (Objects), 110 (Permanents), 111 (Tokens), 112 (Spells), 113 (Abilities), 114 (Emblems), 400–408 (Zones). It does not cover Ante (407).

## Responsibilities
- **The zone-change primitive** `move_object(obj, to, position, cause)`. It:
  - allocates a new object identity (CR 400.7)
  - emits proposed and occurred events (so replacements like Rest in Peace and commander redirection can apply)
  - records last-known information
- **Token and copy lifecycle:** tokens that leave the battlefield cease to exist as an SBA (CR 704.5d), and so do copies of spells or cards outside their zone (CR 704.5e).
- **Library and hidden-zone ordering**, plus position-aware insertion ("third from the top", "bottom in a random order").
- **Object and zone queries** for filters ("each creature you control", "cards in all graveyards").

## Owned data
`ObjectArena`, `Zones`, last-known information snapshots (CR 608.2h), and zone-change history for "this turn" conditions.

## Decisions raised
- Order of cards put onto the top or bottom of a library simultaneously (owner chooses).
- Order of cards put into a graveyard simultaneously (owner chooses; relevant for some old cards).

## Edge cases
- Moving to the **same zone** (e.g. graveyard to graveyard) still creates a new object (CR 400.7).
- **Leaves-the-battlefield look-back** for triggers (CR 603.10).
- An object put into a library or hand with unknown identity breaks other players' knowledge ([[Hidden Information & Player Views]]).
- Split, adventure, MDFC, meld and prepare/omen card layouts have different characteristics per zone (CR 709–722).

## Test scenarios
- Rest in Peace plus a creature dying: it goes to exile, and no "dies" trigger fires.
- A commander going to the graveyard: the owner chooses whether to move it to the command zone (CR 903.9a, an SBA).
- Flickering a creature with an Aura attached: the new object, and the Aura falls off via the SBA (CR 704.5m).

## Open questions
- How much last-known information to snapshot (full characteristics vs. lazily, on demand).

## Related
[[Game State Model]] · [[Replacement & Prevention]] · [[Commander]]
