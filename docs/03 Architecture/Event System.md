---
type: architecture
status: draft
tags: [architecture, events, triggers, metrics]
updated: 2026-10-20
---

# Event System

Events are the engine's single record of "what happened". Five consumers depend on them:

| Consumer | Uses events to… |
|---|---|
| Triggered abilities (CR 603) | Match trigger conditions ("whenever a creature dies…"). |
| Replacement effects (CR 614) | Intercept events **before** they happen. These operate on *proposed* events. |
| Metrics ([[Metrics Catalog]]) | Count casts, draws, damage, wins, etc. without the engine knowing about metrics. |
| Logs and replays | Produce human-readable game logs and annotated replays. |
| Judge explanations | "Rhystic Study triggered because Player 2 cast Brainstorm (CR 603.2)." |

## Two kinds of event
1. **Proposed events** are built by effects *before* they happen. They pass through the replacement/prevention pipeline (CR 614–616), which may modify, replace, or prevent them, possibly asking players to order replacements (CR 616.1). The result is then performed.
2. **Occurred events** are emitted after the state changes. Triggers look at these (CR 603.2), plus "leaves-the-battlefield" look-back (CR 603.10) and state triggers (CR 603.8).

## Event taxonomy (sketch)
- **Zone change:** `{object, from, to, cause}`. Covers dies, ETB, LTB, draw (CR 121), discard, mill, exile, cast (moving to the stack), and so on.
- **Damage:** `{source, recipient, amount, combat: bool}`, which feeds lifelink, deathtouch flags, and commander damage.
- **Life change:** gain or loss (CR 119).
- **Counter change:** put or remove (CR 122).
- **Tap/untap, transform, phase in/out, face-up/face-down.**
- **Cast / activate / trigger / resolve / counter** (CR 601, 602, 603, 608, 701.6).
- **Turn structure:** begin/end of each phase and step (CR 500-series).
- **Player:** lose, win, draw, concede, leave game (CR 104, 800.4).
- **Attack/block declarations** (CR 508, 509).
- **Designation changes:** monarch, initiative, day/night.
- **Search/shuffle/reveal/look:** for knowledge tracking and metrics.

Each event carries `cr: CrRef`, the rule that caused it, for judge explanations.

## Simultaneity
Many events happen "simultaneously", such as SBAs (CR 704.3), a board wipe, or combat damage (CR 510.2). The engine groups them into an **event batch**. Triggers from one batch are collected together and put on the stack in APNAP order with player-chosen ordering (CR 603.3b).

## Metrics hook cost
Metric collection is an `EventSink` trait object configured per game. When all metric flags are off, the sink is a no-op so it costs nothing ([[Non-Functional Requirements]] NFR-P1).

## Related
[[Triggered Abilities]] · [[Replacement & Prevention]] · [[Metrics Catalog]]
