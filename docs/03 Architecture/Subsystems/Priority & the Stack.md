---
type: subsystem
status: draft
cr: ["117", "405"]
impl_status: not-started
milestone: M2
tags: [subsystem, priority, stack]
updated: 2026-10-20
---

# Priority & the Stack

## Purpose
This subsystem decides who may act, when, and in what order spells and abilities resolve.

## CR sections
117 (timing and priority), 405 (stack), plus 608 via [[Resolution]].

## Responsibilities
- The priority loop:
  - before granting priority, perform SBAs repeatedly, then put waiting triggers on the stack (CR 117.5, 704.3, 603.3)
  - the active player gets priority after resolution (CR 117.3b)
  - a player who acts keeps priority (CR 117.3c)
  - passing goes to the next player in turn order (CR 117.3d)
  - **all players pass in succession** → the top object resolves, or the step ends (CR 117.4, 405.5)
- Timing restrictions: sorcery-speed (main phase, own turn, empty stack), instant-speed, "activate only once each turn", split second (CR 702.61), and "can't cast spells" stax (Grand Abolisher, Silence, Drannith Magistrate, Rule of Law).
- Things that don't use the stack (CR 405.6): mana abilities, special actions, SBAs, turn-based actions.

## Decisions raised
- `Priority { actions }`, the main action menu ([[Decision & Action Model]]).

## Edge cases
- **Multiplayer pass-around:** in a 4-player game, all four must pass in succession. One player acting resets it.
- **Holding priority** (acting again before passing) is legal and must be expressible. For example, cast a spell, then respond to your own spell.
- **Mana abilities while you have priority** produce mana that stays in the pool until the step ends.
- **Players who have left the game** are skipped.

## Performance note
In cEDH most priority decisions are "pass". The trivial-decision fast path ([[Decision & Action Model]] R6) and policy auto-pass predicates matter a lot here.

## Test scenarios
- Four-player stack: P1 casts, P3 responds, P2 responds to P3. Resolution is LIFO and priority returns to the active player after each resolution.
- Grand Abolisher on P1's turn: opponents get **no** cast options during P1's turn, though they can still activate non-creature/artifact/enchantment abilities per the Oracle text.

## Related
[[Resolution]] · [[Casting & Activating]] · [[Turn Structure]]
