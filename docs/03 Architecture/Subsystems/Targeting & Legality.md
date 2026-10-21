---
type: subsystem
status: draft
cr: ["115", "608.2b", "702.11", "702.16", "702.18", "702.21"]
impl_status: not-started
milestone: M3
tags: [subsystem, targeting]
updated: 2026-10-20
---

# Targeting & Legality

## Purpose
This subsystem decides which objects and players can be targeted or chosen, and rechecks targets on resolution.

## CR sections
115 (targets), 601.2c (choosing targets), 608.2b (target legality on resolution), and the keywords that interact with targeting: hexproof (702.11), protection (702.16), shroud (702.18) and ward (702.21).

## Responsibilities
- Evaluate **target filters** from the card IR ("target creature an opponent controls", "target spell with mana value 2 or less") against the *current* characteristics (post-layers).
- Enforce restrictions: hexproof, shroud, protection (the DEBT abilities, CR 702.16), "can't be the target of", and Leonin Abuna-style effects.
- Handle the "same object can't be chosen for multiple instances of the word *target*" rule vs. different target words (CR 115.3).
- **Changing targets** (Deflecting Swat, Bolt Bend), and "choose new targets" for copies (CR 707.10c).
- Rechecking on resolution: illegal targets are unaffected, and a spell with all targets illegal doesn't resolve (CR 608.2b).
- Ward triggers (CR 702.21) are **triggered abilities** that counter unless a cost is paid. That's an "unless" payment decision for the targeting player.

## Decisions raised
`ChooseTargets` per target slot (factored), and ward's `ChooseOption` (pay or don't).

## Edge cases
- Protection from a quality changes as characteristics change (layers).
- "Any target" covers creature, player, planeswalker or battle (CR 115.4).
- Modal spells with different targets per mode.
- Spells that target a spell on the stack that leaves before resolution.

## Related
[[Casting & Activating]] · [[Resolution]] · [[Keywords & Keyword Actions]]
