---
type: architecture
status: draft
tags: [architecture, state, core]
updated: 2026-10-20
---

# Game State Model

## Goals
- Representing **every** CR concept is possible. Nothing gets special-cased into "can't happen".
- Cloning is cheap ([[Performance & Cheap Cloning]]), along with hashing (for loop detection and golden replays) and serialization.
- Object identity follows the CR's rules: an object that changes zones becomes a **new object** with no memory of its previous existence (CR 400.7), with the listed exceptions.

## Top-level shape (sketch)
```rust
pub struct GameState {
    pub rules: GameRules,                 // format, starting life, variant options (CR 800-series), CR version
    pub players: Vec<Player>,             // seat order; includes players who have left (flagged)
    pub turn: TurnState,                  // active player, phase/step, turn number, extra turns/phases queue
    pub objects: ObjectArena,             // all game objects (cards, tokens, copies, emblems, abilities-on-stack)
    pub zones: Zones,                     // per-player library/hand/graveyard, shared battlefield/stack/exile/command
    pub effects: ContinuousEffects,       // active continuous effects with timestamps + dependencies (CR 611, 613)
    pub replacements: ReplacementRegistry,// active replacement/prevention effects (CR 614, 615)
    pub pending_triggers: Vec<PendingTrigger>, // triggered but not yet put on stack (CR 603.3)
    pub delayed_triggers: Vec<DelayedTrigger>,  // CR 603.7
    pub combat: Option<CombatState>,      // attackers, blockers, damage assignment state
    pub designations: Designations,       // monarch (725), initiative (726), day/night (731), city's blessing…
    pub turn_history: TurnHistory,        // "this turn" facts: spells cast, life lost, creatures died… (for conditions)
    pub continuation: Continuation,       // where the rules procedure is suspended (see Decision & Action Model)
    pub rng: GameRng,                     // seeded ChaCha stream (see Determinism & Replay)
    pub timestamp_clock: u64,             // CR 613.7
    pub next_ids: IdAllocators,
}
```

## Objects
- **IDs:** `ObjectId` is a generational index into an arena. A zone change allocates a **new** `ObjectId` (CR 400.7) and records `previous: Option<ObjectId>` *only for rules that look back*: last-known information (CR 608.2h, 113.7a), "the card that was exiled with ~" via linked abilities (CR 607), and so on.
- **Characteristics** (CR 109.3) are stored as:
  - `base`: printed or copiable values, from the card IR or a token definition
  - `derived`: computed through the layer system ([[Continuous Effects & Layers]]), cached, and invalidated when effects or the board change
- **Status** (CR 110.5): tapped, flipped, face-down, phased out.
- **Counters** (CR 122), stickers (CR 123), attached-to, damage marked, deathtouch-damage flag (for CR 704.5h), "summoning sick" tracking (CR 302.6 continuous control since the start of the most recent turn).
- **Ownership vs. control** (CR 108.3, 110.2) are separate fields. Control is derived through layer 2.

## Zones (CR 400)
| Zone | Ordered? | Hidden? | Notes |
|---|---|---|---|
| Library (401) | yes | yes | Top-of-library knowledge is tracked per player (revealed, scry'd, Vampiric Tutor'd). |
| Hand (402) | no | yes (to others) | Revealed-card knowledge is tracked. |
| Battlefield (403) | no | no | Face-down permanents are partially hidden (CR 708). |
| Graveyard (404) | yes | no | Order matters for some old cards. |
| Stack (405) | yes | no | Holds spells, abilities and copies. |
| Exile (406) | no | sometimes | Face-down exile (e.g. hideaway, foretell) is hidden. |
| Command (408) | no | no | Commanders, emblems, dungeons, etc. |

## Players
Each player has life, poison, counters (CR 122), mana pool (CR 106.4), lands played this turn, maximum hand size, commander damage taken (keyed by commander identity), designations, "has left the game", team ID (for 2HG and team play), and per-player knowledge tracking for [[Hidden Information & Player Views]].

## Continuations (suspended rules procedures)
Rules procedures like casting (CR 601.2a–i), resolving (CR 608), or declaring blockers are multi-step and may need many decisions. They are written as **explicit state machines** whose position is stored in `GameState.continuation`. The alternative is Rust `async`/generators, rejected for now: they can't be cloned or serialized. That's what lets a game be cloned mid-cast for search, or serialized mid-decision for the MCP server.

Open question: hand-written enum state machines vs. a small internal "rules VM" (bytecode for procedures). See [[12 Open Questions]].

## Hashing
A Zobrist-style or structural hash of the game-relevant state supports:
- loop detection (CR 732, [[Loops & Shortcuts]])
- golden-replay assertions
- MCTS transposition (optional)

## Related
[[Zones & Objects]] · [[Decision & Action Model]] · [[Event System]] · [[Performance & Cheap Cloning]]
