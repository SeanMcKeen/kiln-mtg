---
type: subsystem
status: draft
cr: ["104", "727", "729"]
impl_status: not-started
milestone: M2
tags: [subsystem, game-end]
updated: 2026-10-20
---

# Game End, Restarts & Subgames

## CR sections
104 (ending the game), 727 (restarting the game: Karn Liberated), 729 (subgames: Shahrazad).

## Responsibilities
- **Winning** (CR 104.2): last player standing, or an effect says the player wins (Thassa's Oracle, Laboratory Maniac, Jace, Wielder of Mysteries, Aetherflux Reservoir by damage).
- **Losing** (CR 104.3): concede (104.3a), 0 life (104.3b), drawing from an empty library (104.3c), 10 poison (104.3d), effects (104.3e), win and lose simultaneously means a loss (104.3f), commander damage (104.3j).
- **Draws** (CR 104.4): all remaining players lose simultaneously (104.4a), mandatory loops (104.4b).
- **Game result recording** for simulation: winner(s), turn, **win type** (combat, combo-alt-win, life drain, mill, poison, commander damage, concession, draw-loop, cap-reached). See [[Metrics Catalog]].
- **Simulation caps** (FR-S11): max turns, max decisions, wall time. Hitting a cap is recorded as `capped`, never as a draw-by-rules.
- **Restart** (CR 727) and **subgames** (CR 729): rare. Supported late (M11), but the state model shouldn't preclude nesting.

## Platinum Angel / "can't win/lose"
"Can't lose" and "opponents can't win" effects override the SBAs (CR 101.2: "can't" wins). Test: Thassa's Oracle against an opponent's Platinum Angel.

## Related
[[State-Based Actions]] · [[Metrics Catalog]] · [[Multiplayer]]
