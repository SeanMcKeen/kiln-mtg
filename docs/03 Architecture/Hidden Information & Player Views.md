---
type: architecture
status: draft
tags: [architecture, hidden-information, policies, fairness]
updated: 2026-10-20
---

# Hidden Information & Player Views

## Why this matters
Simulation results are only valid if no policy can see what its player couldn't see. A search AI that peeks at opponents' hands or library order inflates the results. So does an LLM that's shown the full state. Fairness has to be **enforced by construction**: policies receive only a `PlayerView`.

## What a player knows
| Information | Visible to |
|---|---|
| Public zones (battlefield, graveyards, stack, exile face-up, command) | everyone |
| Own hand | owner |
| Others' hands | count only, plus **known cards** (revealed, or tutored to hand while revealed) |
| Libraries | count only, plus known positions (scry, "look at the top", Vampiric Tutor put on top) |
| Face-down permanents and spells (CR 708) | controller can look; others see a 2/2 face-down object |
| Face-down exile (foretell, hideaway, etc.) | per card rules |
| Choices made secretly (CR 101.4a) | hidden until revealed |
| RNG state | nobody |

## Knowledge tracking
The engine maintains, per player, a **knowledge set** over hidden objects: "player P knows object O is card C at location L since event E". Knowledge is created by reveal, look, search-then-reveal, and so on. Knowledge is invalidated by:
- shuffles (CR 701.24 *Shuffle*)
- zone changes into a hidden zone with unknown position
- object identity changes (CR 400.7)

> [!warning] Subtle case
> When a player tutors a card to hand *without* revealing it, opponents know the hand count increased and that a card left the library, but not which card. Search AIs must model this ("one unknown card from the library").

## PlayerView API (sketch)
```rust
pub struct PlayerView<'g> { /* borrows GameState, filters on access */ }
impl PlayerView<'_> {
    fn me(&self) -> PlayerId;
    fn object(&self, id: ObjectRef) -> VisibleObject;   // Hidden { zone, owner } or Known { card, characteristics… }
    fn zone(&self, z: ZoneRef) -> ZoneSummary;          // counts + visible contents
    fn known_cards_in(&self, z: ZoneRef) -> Vec<(Position?, CardId)>;
    fn history(&self) -> &PublicLog;                   // public events only
}
```
- The view is **borrowed and lazy**. It doesn't copy the state, so it's cheap enough to build per decision.
- Serialization of a view (for the MCP, LLM, Python or WASM boundaries) is in [[Output Formats & Report Schema]] and [[C - LLM Live Decisions]].

## Determinization (for search)
[[D - Search AI]] samples hidden information consistent with a `PlayerView`:
- opponents' unknown hand cards and library order are drawn from their **decklist minus known locations**
- this assumes decklists are known (open decklists, which are standard for cEDH testing), or else a configured prior

Whether opponents' decklists are known to a policy is an **experiment-spec flag**, `open_decklists: true|false`. See [[Experiment Spec]].

## Related
[[Randomness]] · [[Policy Interface]] · [[Design Principles]] §5
