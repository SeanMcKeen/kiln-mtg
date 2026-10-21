---
type: subsystem
status: draft
cr: ["506", "507", "508", "509", "510", "511"]
impl_status: not-started
milestone: M2
tags: [subsystem, combat]
updated: 2026-10-20
---

# Combat

## Purpose
This subsystem runs the combat phase: declare attackers and blockers with all restrictions and requirements, assign and deal combat damage, and handle first strike, double strike, and trample.

## CR sections
506–511.

## Responsibilities
- **Declare attackers** (CR 508.1): the active player chooses attackers and, in multiplayer, **which player, planeswalker or battle each attacks** (CR 506.3, 802). This requires:
  - checking restrictions (508.1c)
  - maximizing obeyed requirements without violating restrictions (508.1d)
  - paying attack costs (e.g. Ghostly Prison-style)
- **Declare blockers** (CR 509.1): each defending player. In multiplayer, **each defending player declares blockers in APNAP order** (CR 101.4).
- **Combat damage assignment** (CR 510.1):
  - a blocked creature divides its damage among its blockers as its controller chooses (510.1c; the damage assignment order no longer exists)
  - trample assigns lethal to blockers first, then the excess (702.19b)
  - deathtouch makes 1 damage lethal
- First-strike and double-strike damage steps (CR 510.4, 702.4b).
- Removal from combat (CR 506.4).
- **Commander damage tracking:** combat damage dealt by a commander to a player, accumulated per commander identity ([[Commander]]).
- Extra combat phases (CR 500.8): Najeela, Godo + Helm of the Host.

## Decisions raised
- `DeclareAttacker` (factored, one creature at a time, with target choice).
- `DeclareBlocker` (factored).
- `DivideDamage` (CR 510.1c–d).

## The requirements-maximization problem
CR 508.1d and 509.1c require the declaration to obey **as many requirements as possible** without violating restrictions. With factored decisions, the engine must ensure the *final* declaration is legal. Approach:
1. Compute the maximum number of requirements that can be obeyed. This is a small constraint problem, typically trivial.
2. Offer only per-creature options that keep a maximal-legal completion reachable.

It's rare in cEDH, but required for correctness. Test it with "attacks each combat if able" plus "can't attack unless you pay" combinations.

## Test scenarios
- Najeela (Warrior tokens, extra combat via the activated ability), with multiple defenders.
- Tymna: card draw per opponent dealt combat damage.
- A deathtouch + trample assignment.
- A commander-damage loss at exactly 21 with Rograkh partner splits (separate tallies).

## Related
[[Turn Structure]] · [[Commander]] · [[Multiplayer]] · [[Decision & Action Model]]
