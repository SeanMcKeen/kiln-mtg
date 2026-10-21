---
type: subsystem
status: draft
cr: ["500", "501", "502", "503", "504", "505", "506", "507", "512", "513", "514", "703", "724"]
impl_status: not-started
milestone: M2
tags: [subsystem, turn-structure]
updated: 2026-10-20
---

# Turn Structure

## Purpose
This subsystem advances phases and steps, performs turn-based actions (CR 703), and manages extra turns, phases and steps plus ending the turn (CR 724).

## CR sections
500–514 (turn structure; combat steps 506–511 are in [[Combat]]), 703 (turn-based actions), 724 (ending turns and phases).

## Responsibilities
- Phase and step progression, including **skipped steps**: no first-strike damage step without first or double strike (CR 510.4), the draw skip on turn 1 for the starting player in two-player games only, and "skip your draw step" effects.
- Turn-based actions:
  - untap (CR 502.3)
  - draw
  - declare attackers and blockers
  - combat damage
  - cleanup discard to hand size (CR 514.1)
  - damage wear-off
  - "until end of turn" effects ending
- **Cleanup priority exception** (CR 514.3a): if SBAs or triggers occur in cleanup, players get priority and another cleanup step follows.
- **Extra turns, phases and steps queue** (CR 500.7–500.8): Time Walk effects and extra combats. The most recently created extra turn or phase happens first.
- "Until" effects expire as a step or phase begins or ends (CR 500.4–500.5). Mana pools empty at the end of each step and phase (CR 500.5).
- "At the beginning of …" trigger windows.
- **Turn control** (CR 723) hooks in through [[Control & Ownership]].

## Decisions raised
- Discard to hand size in cleanup (which cards).
- Untap choices when effects say "may untap" or restrict untapping.

## Edge cases
- Ending the turn (CR 724): exile the stack, then go to cleanup. Relevant for Sundial of the Infinite and Time Stop.
- Players who leave mid-turn (CR 800.4). If the active player leaves, the turn proceeds without them (see [[Multiplayer]]).

## Test scenarios
- Upkeep triggers (Mystic Remora cumulative upkeep, Pact of Negation's delayed trigger) fire at the correct step.
- A turn-1 draw step skip happens only in two-player games (CR 103.8a).
- Cleanup with a discard that triggers something: priority is given and a new cleanup step follows.

## Related
[[Priority & the Stack]] · [[Combat]] · [[Triggered Abilities]]
