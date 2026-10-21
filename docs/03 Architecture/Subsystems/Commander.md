---
type: subsystem
status: draft
cr: ["903", "408", "704.6c"]
impl_status: not-started
milestone: M6
tags: [subsystem, commander, cedh]
updated: 2026-10-20
---

# Commander

## Purpose
This subsystem implements the Commander variant rules. It's the **primary format** ([[ADR-0010 Commander-First N-Player Core]]).

## CR sections (verified against the 2026-06-19 text)
| Rule | Content |
|---|---|
| 903.2 | Default multiplayer setup: free-for-all, attack multiple players, no limited range of influence |
| 903.3 | Commander designation (legendary creature / Vehicle / Spacecraft with P/T). The designation is **not a characteristic** |
| 903.4 | Color identity (903.4a–f, including DFC back faces and adventurer characteristics) |
| 903.5 | Deck construction: exactly 100 cards, singleton, color identity, no sideboard |
| 903.6 | Commander starts in the command zone |
| 903.8 | Cast from the command zone; **commander tax** of {2} per previous cast from there |
| 903.9a | SBA: commander in graveyard or exile since the last SBA check → owner **may** move it to the command zone |
| 903.9b | Replacement: commander would go to hand or library → owner **may** put it into the command zone instead. It can apply repeatedly (exception to 614.5) |
| 903.10a / 704.6c | 21 or more combat damage from the same commander → loss |
| 903.11 | No cards from outside the game, except via specific effects |
| 903.12 | Brawl option |
| 702.124 | Partner variants (deck-construction keyword) |
| 103.5c | Multiplayer: the first mulligan is free |

## Responsibilities
- Commander designation stored per player, as an attribute of the *card*, not of the object's characteristics. It survives zone changes.
- Commander tax counter per commander per player.
- Commander damage matrix `[commander identity][player]`.
- Deck validation, including color identity and the banned list (the format-legality data source is [[ADR-0009 Card Data Source of Truth]]).
- "Your commander" references (CR 903.3d–e): Fierce Guardianship's "if you control a commander", Command Beacon, and similar.

## Decisions raised
`CommanderZoneChange`, for both the 903.9a SBA and the 903.9b replacement.

## Banned list awareness
As of the 2026-10-20 Scryfall data, **Mana Crypt, Dockside Extortionist, Jeweled Lotus and Hullbreacher** are banned in Commander. They're useful negative test cases for deck validation. Banlists change, so the legality data comes from the card DB, never hard-coded.

## Related
[[Multiplayer]] · [[Combat]] · [[Replacement & Prevention]] · [[Formats & Game Modes]]
