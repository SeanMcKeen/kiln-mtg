---
type: subsystem
status: draft
cr: ["108.3", "110.2", "613.1b", "723", "800.4"]
impl_status: implemented
milestone: M5
tags: [subsystem, control]
updated: 2026-10-23
---

# Control & Ownership

## Purpose
This subsystem keeps track of who owns and who controls each object, control-changing effects (layer 2), and **controlling another player** (CR 723).

## Responsibilities
- Owner is immutable, apart from rare exceptions. Controller is derived through layer 2 ([[Continuous Effects & Layers]]).
- Summoning sickness tracking relative to the controller (CR 302.6).
- **Controlling another player** (CR 723). Two cards let a player control another player *for a limited duration*, and **Opposition Agent**, a cEDH staple, is one of them (CR 723.2). The controller makes all choices and decisions for the controlled player (CR 723.5). In the decision model, `PendingDecision.player` is the **controlling** player, while effects still treat the controlled player as the actor. Policies must receive the **controlling player's view**, plus whatever the controlling player is allowed to see.
- Mindslaver / Emrakul-style full-turn control (CR 723.1).
- Leaving the game: control effects end (CR 800.4a), and the control fallback rules apply (CR 800.4b–c).

## Decisions raised
Re-routes the `player` of any decision made by a controlled player.

## Edge cases
- **Opposition Agent:** "You control your opponents while they're searching their libraries", plus exiling the cards they find. Requires the decision router to switch the decider mid-resolution of an *opponent's* tutor.
- Gilded Drake / exchange-of-control effects.
- Stolen commanders: still the owner's commander for tax, zone redirection and commander damage.

## Related
[[Multiplayer]] · [[Hidden Information & Player Views]] · [[Decision & Action Model]]
