---
type: subsystem
status: draft
cr: ["104.4b", "104.4f", "732"]
impl_status: not-started
milestone: M4
tags: [subsystem, loops, shortcuts, cedh]
updated: 2026-10-20
---

# Loops & Shortcuts

> [!important] Critical for cEDH
> Many cEDH wins are infinite or very large loops: Dramatic Reversal + Isochron Scepter, Dualcaster Mage + Twinflame, Kiki-Jiki + Zealous Conscripts, Walking Ballista + Heliod, Basalt Monolith + Rings of Brighthearth, Underworld Breach + LED + Brain Freeze, Food Chain + Eternal Scourge. A simulator that executes these one iteration at a time is slow. One that can't recognize them can't finish games.

## CR sections
104.4b (mandatory loops are a draw), 104.4f (range-of-influence variant), 732 (taking shortcuts): 732.2a proposing, 732.2b–c accepting or shortening, 732.3 fragmented loops, 732.4 mandatory-only loop is a draw, 732.5 no forced loop-breaking, 732.6 "unless" in loops.

## Responsibilities
1. **Loop detection:** detect that the game state, modulo *monotonic counters* (mana in pool, life totals, number of tokens, storm count, damage dealt), has repeated after a sequence of choices by the same player(s). Use state hashing that excludes the monotonic quantities, plus a delta vector of those quantities.
2. **Shortcut proposal (CR 732.2a):** when a policy, or the engine on a policy's behalf, recognizes a loop, it proposes "repeat N times". Other players may accept or shorten it, naming where they'll act (CR 732.2b), in turn order.
3. **Shortcut execution:** apply the delta vector × N in one step, where it's provably linear (each iteration has an identical delta and no triggers whose behaviour changes). Fall back to iterating when it isn't.
4. **Mandatory-loop draw** (CR 104.4b, 732.4): Worldgorger Dragon + Animate Dead with no way out, which also makes a great test.
5. **Large numbers:** "arbitrarily large" quantities need a representation. See FR-E11 and [[12 Open Questions]].

## Decisions raised
- `ProposeShortcut`: choose N, accept, or shorten.
- Policies must be able to say "loop this until X", e.g. "until opponents are dead" or "until I have 1,000 mana".

## Design options
| Option | Pros | Cons |
|---|---|---|
| A. **Engine-detected loops** (state hash repeats) | Generic. Works for any card combo | Needs careful "modulo monotonic quantities" hashing. Detection costs a hash per action |
| B. **Card-IR-annotated combos** (known combo patterns) | Fast, precise | Doesn't generalize. Maintenance burden |
| C. **Policy-declared loops** (the policy says "I'm doing this N times" and the engine verifies one iteration) | Matches CR 732.2a exactly. Cheap | The policy must know the loop |

**Leaning: A + C.** The engine detects a repeat and offers a shortcut decision. Playbooks and search can also proactively declare loops. B is only an optimization hint.

## Test scenarios
- Worldgorger + Animate Dead with no other actions → draw (104.4b).
- Dramatic Reversal + Isochron Scepter + Sol Ring + Mana Vault → arbitrarily large mana. A shortcut to N = 1,000.
- Opponent shortens a shortcut to respond mid-loop (732.2b).

## Related
[[Costs & Mana]] · [[Tricky Interactions Catalog]] · [[MTG Complexity & Loops]]
