---
type: subsystem
status: draft
cr: ["603"]
impl_status: not-started
milestone: M4
tags: [subsystem, triggers]
updated: 2026-10-20
---

# Triggered Abilities

## Purpose
This subsystem detects triggers, queues them, and puts them on the stack in the correct order **with player-chosen ordering**.

## CR sections
603 (handling triggered abilities): trigger events (603.2), putting triggers on the stack (603.3), intervening "if" (603.4), delayed triggers (603.7), state triggers (603.8), look-back for leaves-the-battlefield and similar (603.10).

## Responsibilities
- Match occurred events against active trigger conditions. Index conditions by event type for speed ([[Performance & Cheap Cloning]]).
- **Intervening "if" clauses** (CR 603.4): check on trigger *and* on resolution.
- Queue pending triggers. The next time a player would receive priority, put them on the stack: **APNAP order, each player orders their own** (CR 603.3b). Abilities that trigger on other abilities triggering go in a second pass, also in APNAP order (the two-part process in CR 603.3b).
- Delayed triggers (Pact of Negation's "at the beginning of your next upkeep", Mana Drain's mana in the next main phase).
- State triggers (CR 603.8) trigger once until resolved or countered.
- "Leaves the battlefield" and "dies" triggers use look-back (CR 603.10).
- Targets for triggered abilities are chosen as they're put on the stack. If no legal targets exist, the ability is removed (CR 603.3d).
- Modal triggers choose modes at that time (CR 603.3c).

## Decisions raised
- **`OrderTriggers`** per player, the user's explicit example of a customizable legal choice.
- Targets, modes, and "may" choices for each trigger.

## Edge cases (cEDH)
- **Rhystic Study / Mystic Remora / Esper Sentinel / Orcish Bowmasters** all trigger off opponents' actions. Several players' triggers go on the stack in APNAP order.
- **Orcish Bowmasters** triggers on draws except the first draw in each player's draw step. Wheels like Windfall or Timetwister give a massive trigger count, so batch performance matters.
- **Notion Thief** is a replacement, not a trigger. Mixed with Bowmasters triggers, it's a classic test.
- Triggers whose controller has left the game cease (CR 800.4d).

## Test scenarios
- Two triggers controlled by P1 and one controlled by P3, on P2's turn: the order on the stack is P2's (none), then P3's, then P4's, then P1's. **Last put on the stack resolves first.**
- The intervening-if fails on resolution, so the trigger does nothing.

## Related
[[Event System]] · [[Priority & the Stack]] · [[Replacement & Prevention]]
