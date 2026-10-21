---
type: reference
status: draft
tags: [rules-map, cr]
cr_version: 2026-06-19
updated: 2026-10-20
---

# CR → Subsystem Map

Every CR section has exactly one **owning** subsystem, with secondary touch-points noted. Priority: **P0** = needed for the cEDH MVP (M6), **P1** = needed for the full card pool, **P2** = other formats or rare, **✗** = not planned.

| CR | Title | Owner | Priority | Notes |
|---|---|---|---|---|
| 100–101 | General, Golden Rules | [[Architecture Overview]] | P0 | 101.1 card-over-rules, 101.2 "can't" wins, 101.4 APNAP |
| 102 | Players | [[Multiplayer]] | P0 | Teams (102.4) |
| 103 | Starting the Game | [[Turn Structure]] | P0 | Mulligans (103.5, 103.5c multiplayer free mulligan) |
| 104 | Ending the Game | [[Game End, Restarts & Subgames]] | P0 | |
| 105–107 | Colors, Mana, Numbers & Symbols | [[Costs & Mana]] | P0 | |
| 108–114 | Cards, Objects, Permanents, Tokens, Spells, Abilities, Emblems | [[Zones & Objects]] | P0 | |
| 115 | Targets | [[Targeting & Legality]] | P0 | |
| 116 | Special Actions | [[Casting & Activating]] | P0 | Land play, morph, suspend… |
| 117 | Timing and Priority | [[Priority & the Stack]] | P0 | |
| 118 | Costs | [[Costs & Mana]] | P0 | |
| 119–121 | Life, Damage, Drawing | [[Resolution]] | P0 | Events in [[Event System]] |
| 122–123 | Counters, Stickers | [[Counters]] | P0 / P2 | Stickers are Un-adjacent (Unfinity) |
| 200–213 | Parts of a Card | [[Card Data Model]] | P0 | Ingestion maps Oracle fields |
| 300–310 | Card Types (incl. Kindred, Dungeon, Battle) | [[Card Data Model]] + per-type rules in subsystems | P0–P1 | |
| 311–315 | Planes, Phenomena, Vanguard, Schemes, Conspiracies | — | ✗ | Casual variants |
| 400–406, 408 | Zones | [[Zones & Objects]] | P0 | |
| 407 | Ante | — | ✗ | |
| 500–505, 512–514 | Turn structure | [[Turn Structure]] | P0 | |
| 506–511 | Combat | [[Combat]] | P0 | |
| 600–602, 606–607 | Casting, activating, loyalty, linked | [[Casting & Activating]] | P0 | |
| 603 | Triggered abilities | [[Triggered Abilities]] | P0 | |
| 604, 611–613 | Static and continuous effects, layers | [[Continuous Effects & Layers]] | P0 | |
| 605 | Mana abilities | [[Costs & Mana]] | P0 | |
| 608–610 | Resolution, effects | [[Resolution]] | P0 | |
| 614–616 | Replacement and prevention | [[Replacement & Prevention]] | P0 | |
| 700 | General (modal, "dies", etc.) | [[Resolution]] | P0 | 700.2 modes |
| 701 | Keyword actions (69) | [[Keywords & Keyword Actions]] | P0–P1 | |
| 702 | Keyword abilities (194) | [[Keywords & Keyword Actions]] | P0–P1 | |
| 703 | Turn-based actions | [[Turn Structure]] | P0 | |
| 704 | State-based actions | [[State-Based Actions]] | P0 | |
| 705–706 | Coins, dice | [[Randomness]] | P1 | |
| 707 | Copying | [[Copy Effects]] | P0 | |
| 708 | Face-down | [[Zones & Objects]] + [[Continuous Effects & Layers]] | P1 | Morph, manifest, cloak, disguise |
| 709–722 | Split, flip, leveler, DFC, substitute, saga, adventurer, class, attraction, prototype, case, omen, station, preparation | [[Card Data Model]] + [[Casting & Activating]] | P1 (attractions P2) | MDFC lands matter in cEDH |
| 723 | Controlling another player | [[Control & Ownership]] | **P0** | Opposition Agent (cEDH staple) |
| 724 | Ending turns and phases | [[Turn Structure]] | P1 | |
| 725–726 | Monarch, initiative | [[Multiplayer]] | P1 | |
| 727 | Restarting the game | [[Game End, Restarts & Subgames]] | P2 | Karn Liberated |
| 728 | Rad counters | [[Counters]] | P1 | |
| 729 | Subgames | [[Game End, Restarts & Subgames]] | P2 | Shahrazad (banned nearly everywhere) |
| 730 | Merging (mutate) | [[Zones & Objects]] | P1 | |
| 731 | Day and night | [[Turn Structure]] | P1 | |
| 732 | Taking shortcuts | [[Loops & Shortcuts]] | **P0** | cEDH combos |
| 733 | Handling illegal actions | [[Casting & Activating]] | P0 | Rewind semantics |
| 800–806 | Multiplayer options, free-for-all | [[Multiplayer]] | P0 (801, 803, 804 P2) | |
| 805, 810 | Shared turns, 2HG | [[Two-Headed Giant]] | P2 | |
| 807–809, 811 | Grand Melee, Team vs. Team, Emperor, Alternating Teams | [[Multiplayer]] | P2 | |
| 903 | Commander | [[Commander]] | **P0** | |
| 900–902, 904–905 | Other casual variants | — | ✗ | |

## Related
[[CR Structure]] · [[Subsystems Index]] · `Rules Coverage.base`
