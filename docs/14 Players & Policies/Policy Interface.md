---
type: design
status: proposed
tags: [policies, api]
updated: 2026-10-20
---

# Policy Interface

All decision-makers share **one interface** (FR-P1, [[ADR-0006 Pluggable Decision-Makers]]).

```rust
// Sketch
pub trait Policy: Send {
    /// Called once per game, before mulligans.
    fn begin_game(&mut self, ctx: &GameContext, seat: PlayerId, rng_seed: u64) {}
    /// Answer one decision. `view` is redacted to what `seat` may know.
    fn decide(&mut self, view: &PlayerView, decision: &PendingDecision) -> Choice;
    /// Optional fast path: "I'd pass here", without a full decide() call.
    fn auto_pass(&self, view: &PlayerView, decision: &PendingDecision) -> bool { false }
    /// Observe public events (for opponent modelling, logging).
    fn observe(&mut self, event: &PublicEvent) {}
    fn end_game(&mut self, result: &GameResult) {}
    /// Optional rationale for the last decision (LLM/search/playbook), stored in annotated replays.
    fn rationale(&self) -> Option<String> { None }
}
```

## Rules
- **Policies see only `PlayerView`.** No access to `GameState` (FR-P6, [[Hidden Information & Player Views]]).
- The search AI needs to *simulate*. It receives a **determinization service**: "give me a full `GameState` sampled consistently with this view, using this RNG". Sampling happens on the engine side, so hidden information never leaks. Only plausible fabrications are produced.
- Policies are deterministic given their seed, except LLM policies ([[Reproducibility]]).
- Policies return a `Choice` that refers to an offered option. Anything else is a bug and aborts the game with a diagnostic.

## Related
[[Decision & Action Model]] · [[Policy Composition]]
