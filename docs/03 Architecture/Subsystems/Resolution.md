---
type: subsystem
status: draft
cr: ["608", "609", "610"]
impl_status: not-started
milestone: M2
tags: [subsystem, resolution, effects]
updated: 2026-10-20
---

# Resolution

## Purpose
This subsystem resolves the top object of the stack: it checks targets, follows instructions in order, and applies one-shot effects or creates continuous ones.

## CR sections
608 (resolving spells and abilities), 609 (effects), 610 (one-shot effects).

## Responsibilities
- Target recheck (CR 608.2b), then execute the effect IR **in written order** (CR 608.2c), with choices made at resolution (CR 608.2d–e).
- **Information determined once** at application (CR 608.2h), and last-known information for objects that have left.
- One-shot effects that create **delayed triggers** (CR 603.7) or **continuous effects** with durations.
- A permanent spell becomes a permanent (CR 608.3). An instant or sorcery goes to its owner's graveyard as the final step (CR 608.2n), unless an effect such as flashback or escape replaces that.
- "Unless" costs paid during resolution (Rhystic Study, Mana Leak, Smothering Tithe): an opponent's `ChooseOption` decision.

## Decisions raised
All choices made during resolution: `ChooseObjects` (search library, sacrifice), `ChoosePlayer`, `ChooseOption` (name a card for Demonic Consultation, choose a color), `OrderObjects` (scry, surveil, Brainstorm put-back), `YesNo` ("you may").

## Effect interpreter
Effects are interpreted from the card IR ([[ADR-0007 Card Definition Approach]]). The interpreter is a **resumable state machine** because an effect can need decisions partway through ("search your library for a card, put it into your hand, then shuffle").

## Edge cases
- **Demonic Consultation:** name a card, exile the top six, then reveal until the named card (it may exile the whole library). A big hidden-information interaction with Thassa's Oracle.
- **Ad Nauseam:** a repeated "may" loop that reveals and loses life. Each repetition is a decision.
- **Doomsday:** choose 5 cards from library and graveyard in a chosen order. Huge choice space, so factor it.

## Related
[[Priority & the Stack]] · [[Triggered Abilities]] · [[Continuous Effects & Layers]]
