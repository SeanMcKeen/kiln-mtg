---
type: subsystem
status: draft
cr: ["800", "801", "802", "803", "806"]
impl_status: in-progress
milestone: M6
tags: [subsystem, multiplayer]
updated: 2026-10-23
---

# Multiplayer

## Purpose
This subsystem handles N-player games: turn order, attacking multiple players, players leaving the game, and range of influence.

## CR sections
800 (general), 801 (limited range of influence), 802 (attack multiple players), 803 (attack left/right), 806 (free-for-all).

## Responsibilities
- **N-player core from day one** ([[ADR-0010 Commander-First N-Player Core]]). No logic should assume "the opponent", singular. "Each opponent", "target opponent", "an opponent", "the player to your left" are all resolved properly.
- APNAP ordering (CR 101.4) for every simultaneous choice and for trigger placement.
- **Attack multiple players** (CR 802), the Commander default (CR 903.2).
- **Leaving the game** (CR 800.4a–i), which is critical in cEDH, where one player's loss or concession happens mid-game:
  - all objects owned by the leaving player leave
  - control effects that give them control end
  - objects on the stack they controlled cease to exist
  - remaining objects they control are exiled
  - choices they would make are re-routed (800.4g–h)
  - costs they'd pay aren't paid (800.4f)
  - combat damage to them isn't assigned (800.4e)
- If the active player leaves, the turn continues without an active player until it ends (check CR 800.4 sub-rules during implementation).
- **Range of influence** (CR 801) as a model concept. It's off for Commander, so implementation is lower priority.

## Decisions raised
Re-routed choices from players who've left.

## Implementation (M6 6.2)
- Leaving (800.4a): owned objects leave; control effects giving the player control end, returning control to the latest remaining effect or the earlier controller; their abilities on the stack, pending triggers and delayed triggers cease to exist; anything they still control is exiled.
- Concession (104.3a) is `Game::concede(player)`, callable at any time (M6 plan C8). The pending decision is derived again; a procedure that now needs a player who left, or has no legal option, ends. Replays record concessions. The fuzzer can concede at random (`--concede`, per mille).
- "You can't lose the game" / "Your opponents can't win the game" are player rules, from effects or static abilities (Platinum Angel). A player who can't lose isn't an SBA candidate; concession and last-player-standing (104.2a) still apply.
- Exchange of control (701.12a–b) for Gilded Drake.
- Already in place since M2: 800.4d (their triggers), 800.4e (no combat damage to them), 800.4j–k (turns without an active player, skipped turns), and invariant 2 (no decision goes to a player who left).
- Gaps: [[12 Open Questions]] Q29.

## Edge cases
- Thassa's Oracle wins the game for one player, so everyone else loses (CR 104.2).
- Simultaneous losses (all remaining players lose) give a draw (CR 104.4a).
- A player who loses during their own turn: the turn continues, with no active player for some purposes.
- A Gilded Drake'd creature whose controller leaves.

## Related
[[Commander]] · [[Control & Ownership]] · [[Formats & Game Modes]]
