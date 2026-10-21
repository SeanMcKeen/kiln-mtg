---
type: subsystem
status: draft
cr: ["805", "810"]
impl_status: not-started
milestone: M12
tags: [subsystem, 2hg, team]
updated: 2026-10-20
---

# Two-Headed Giant

## Purpose
This subsystem covers team play with shared turns and a shared life total. It's deferred to M12, but the core model must not preclude it.

## CR sections
805 (shared team turns), 810 (2HG variant).

## Model implications (design now, build later)
- **Teams as a first-class concept:** `team_id` on players, "your team" (CR 102.4), and team loss (CR 104.3g).
- **Shared team turns** (CR 805.4): the *team* takes turns, so "active player" becomes "active team" in some rules. Priority and APNAP order adapt accordingly.
- **Shared life total** (CR 810.9): damage and life changes happen to individuals but apply to the shared total.
- **Shared poison** (CR 810.10), with a team threshold of 15. The standard 10-poison SBA is ignored in 2HG (CR 704.5c).
- **Combat** uses the shared-team-turns combat rules (CR 805.10, per 810.7, a recent change).

## Related
[[Multiplayer]] · [[Formats & Game Modes]]
