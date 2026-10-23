---
type: design
status: implemented-v1
tags: [testing, format, judge]
updated: 2026-10-21
---

# Scenario File Format

The same format serves **tests**, the **judge tool**, and **MCP `mtg_rules_scenario`**. Version 1 is implemented in M2 (`mtg-testkit::scenario`). The scenarios live in `tests/scenarios/*.yaml` and run in CI (`crates/mtg-testkit/tests/scenarios.rs`). YAML is parsed with `yaml_serde`, which is maintained by the YAML organization.

```yaml
scenario: mtg-scenario/1
name: Trample assigns lethal to the blocker, the rest to the player
cr: ["702.19b", "510.1c", "510.2"]       # must exist in the CR (checked by the test)
players:
  - battlefield: [Test Rhino]
  - battlefield: [Test Bird]
turn: { number: 3, active: P1, step: declare_attackers }
script:
  - choose: Test Rhino attacks P2         # matched by option label
  - pass_all                              # every player in the game passes once (CR 117.4)
  - { player: P2, choose: Test Bird blocks Test Rhino (P1) }
  - pass_all
  - check:                                # mid-script expectations
      decision:
        player: P1
        kind: assign_damage
        options: ["1 damage to Test Bird (P2)", "1 damage to P2"]
  - choose: 1 damage to P2
  - choose: 1 damage to P2
  - choose: 1 damage to P2
expect:
  life: { P2: 17 }
  zones: { P2.graveyard: [Test Bird] }
```

## Setup
- `players[]`: `life`, `hand`, `library` (top first), `graveyard` and `battlefield`. A battlefield entry is either a card name or `{ card, tapped, damage, sick, counters }`, where `sick` means summoning sick (CR 302.6).
- `players[].commanders` (M6): a name starts in the command zone (CR 903.6), or `{ card, zone, casts, tapped, sick }` with `zone` one of `command` (default), `battlefield`, `hand`, `graveyard`, `exile`, and `casts` the times already cast from the command zone (the tax, CR 903.8). Quote names containing commas.
- **An omitted `library` holds 20 filler lands ("Test Wastes")**, so draws don't end the game. `library: []` means an empty library.
- `turn`: `number`, `active` (`P1`…), and `step`. The game begins at that step and performs its turn-based actions first. Combat scenarios start at `declare_attackers` and script the attacks.
- Card names are resolved in the card pool. In M2 that's the synthetic test set (`CardPool::test_set`). An unknown name is an error.
- Mulligans are skipped. Game-start rules (CR 103) are tested in Rust (`crates/mtg-engine/tests/setup_rules.rs`).

## Script
| Step | Meaning |
|---|---|
| `choose: <label>` | Pick the option with that exact label. An unknown label is an error that lists the available labels. |
| `{ player: P2, choose: … }` | The same, also asserting who decides. |
| `pass_all` | Each player still in the game passes priority once. |
| `pass: N` | Pass priority N times. |
| `check: { … }` | Check expectations at this point. |

Option labels come from `Game::option_label`, for example:
- "Cast Test Bolt", "Play Test Forest", "Tap Test Forest for {G}", "Pass"
- "Target Test Bear (P2)", "Pay with Test Mountain, Test Forest", "Pay manually", "Spend {R}"
- "Test Bear attacks P3", "Test Wall blocks Test Bear (P1)", "1 damage to P2"
- "Discard Test Bolt", "Keep Test Champion [2]" (for same-name permanents, `[k]` is the 1-based position)
- "Put Test Commander into the command zone", "Leave Test Commander in graveyard", "Put Test Commander into hand" (CR 903.9a–b, kind `commander_zone`)

## Expectations
- `result`: `in_progress`, `draw` or `win P2`.
- `life`: per player.
- `zones`: `P1.hand`, `P1.library`, `P1.graveyard`, `P1.battlefield` (permanents P1 controls), `stack`, `exile`, `command`. Each is compared as a multiset of card names.
- `commander_damage`: player → commander name → combat damage dealt (CR 903.10a).
- `tapped`: card name → expected state.
- `step`: the current step.
- `decision`: `player`, `kind` (`priority`, `declare_blocker`, `assign_damage`, …), `options` (exact, in order), `options_include` and `options_exclude`.

## Not yet
- Board-state attributes: `attached_to`, `face_down`, `controller`.
- `events_include` / `events_exclude`.
- Real card names, which need the M3 card IR.
