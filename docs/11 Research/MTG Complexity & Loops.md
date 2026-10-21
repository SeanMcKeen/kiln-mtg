---
type: research
status: draft
tags: [research, theory, loops]
updated: 2026-10-20
---

# MTG Complexity & Loops

## Results
- **"Magic: The Gathering is Turing Complete"** (Churchill, Biderman & Herrick, 2019). It constructs a game state whose outcome encodes a Turing machine's halting, so **determining the outcome of an MTG game is undecidable** in general. https://arxiv.org/abs/1904.09828
- **"Magic: the Gathering is as Hard as Arithmetic"** (Biderman, 2020) extends the complexity results. https://arxiv.org/abs/2003.05119

## Implications for the engine
1. **No engine can always detect loops or decide outcomes.** Our loop detection ([[Loops & Shortcuts]]) is a *practical* tool for common combos, not a decision procedure.
2. **Caps are necessary** (FR-S11). Games that hit turn, decision or time caps are recorded as `capped`.
3. **Shortcuts are a CR concept** (CR 732). Use them, rather than trying to "solve" loops: the policy proposes N iterations, and the engine verifies one iteration and extrapolates when it's provably linear.
4. **Arbitrarily large numbers** (FR-E11) need a representation. Options:
   - `u64` saturating with a "very large" flag
   - `BigUint` only in pools and counters
   - symbolic "N iterations" values

   This is open ([[12 Open Questions]]).

## Sources
- arXiv 1904.09828, arXiv 2003.05119
- CR 104.4b, 732 (local CR 2026-06-19)
