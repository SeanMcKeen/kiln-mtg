---
type: design
status: draft
tags: [testing, invariants, property]
updated: 2026-10-20
---

# Invariants & Property Tests

These invariants are checked after every `apply()` in debug and fuzz builds:

1. **Every object is in exactly one zone.** No dangling IDs.
2. **The pending decision is well-formed:** non-empty options, and the deciding player is in the game.
3. **Legality round-trip:** every offered option, applied to a clone, succeeds without an internal "illegal" error.
4. **Determinism:** `clone().apply(c)` ≡ `apply(c)` (same hash). Replaying the log reproduces the hash.
5. **Hidden-information safety:** `view_for(p)` serialization contains no hidden card identities that `p` doesn't know (checked by a redaction auditor).
6. **Conservation:** the total card count per owner is constant, except for tokens and copies, card-creating effects, and players leaving.
7. **SBA fixpoint:** after the SBA check, no SBA condition holds.
8. **Mana sanity:** pools empty at step boundaries unless an effect says otherwise.
9. **Monotonic IDs:** a zone change always yields a new object ID (CR 400.7).

`proptest` strategies generate random but legal mid-game states from scenario templates and random-policy prefixes.
