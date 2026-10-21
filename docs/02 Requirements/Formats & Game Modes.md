---
type: requirements
status: accepted
tags: [requirements, formats, commander]
updated: 2026-10-20
---

# Formats & Game Modes

Decision: **Commander-first, with an N-player core from day one.** See [[ADR-0010 Commander-First N-Player Core]].

## Priority order
| Priority | Format / variant | CR | Milestone | Notes |
|---|---|---|---|---|
| **1** | **Commander, 4-player free-for-all (cEDH)** | 806, 903 | [[M6 Commander & cEDH Core Pool]] | The headline use case. |
| 1 | Commander 1v1 / Duel Commander-style | 903 | M6 | Mostly free once multiplayer works. Different banlist. |
| 2 | 1v1 constructed (Standard, Modern, Legacy, Vintage, Pauper, …) | 100–733 | [[M12 More Formats & Bindings]] | Rules are a subset of multiplayer. Mostly deck-validation work. |
| 3 | Two-Headed Giant | 810 | M12 | Shared life total, shared turns (805), team poison of 15. |
| 3 | Other team variants (Team vs. Team, Emperor, Alternating Teams) | 808, 809, 811 | M12+ | Range of influence (801) is needed for Emperor. |
| 4 | Limited (sealed/draft pools, 40-card decks) | 100.2b | M12 | Pool generation and draft simulation sit *outside* the rules engine. |
| — | Brawl | 903.12 | opportunistic | A small delta from Commander. |
| ✗ | Planechase, Vanguard, Archenemy, Conspiracy, Commander Draft | 901–905 | not planned | Not precluded architecturally. |

## Commander specifics the engine must model
Rule numbers are verified against the CR text dated 2026-06-19.
- **Command zone** (CR 408) and casting a commander from it, plus **commander tax**: an additional {2} per previous cast from the command zone (CR 903.8).
- **Commander zone-change replacement:** "may put it into the command zone instead" when it would go to hand or library (CR 903.9b), and the state-based option from graveyard or exile (CR 903.9a). Both are player **choices**.
- **Commander damage:** 21 or more combat damage from the same commander means a loss (CR 903.10a; SBA CR 704.6c). This is tracked per (commander object identity, damaged player).
- **Color identity** (CR 903.4) for deck validation and for effects that reference it.
- **Deck construction** (CR 903.5): exactly 100 cards, singleton except basics, color identity.
- **Multiplayer mulligan:** the first mulligan is free (CR 103.5c).
- **Partner / Friends forever / Background / Doctor's companion** commander pairs: designations, plus two commander-damage tallies.
- **Starting life** of 40.
- **Leaving the game** (CR 800.4a): objects owned by the leaving player leave, control effects end, and so on. This is critical in 4-player pods.

## Multiplayer options
- **Free-for-all** (CR 806) with **attack multiple players** (CR 802), which is standard in Commander.
- **Limited range of influence** (CR 801) is *not* used in standard Commander, but the engine models range of influence so that Emperor and large variants are possible later.

## Turn order and seating
- The starting player is chosen randomly by seeded RNG, or set by the experiment spec ([[Experiment Spec]]).
- Seat position strongly affects cEDH win rates. The simulation harness rotates or balances seating ([[Statistics for Multiplayer Simulation]]).

## Related
[[Commander]] · [[Multiplayer]] · [[Two-Headed Giant]] · [[Scope & Non-Goals]]
