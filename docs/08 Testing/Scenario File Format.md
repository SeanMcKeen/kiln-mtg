---
type: design
status: draft
tags: [testing, format, judge]
updated: 2026-10-22
---

# Scenario File Format (draft)

The same format serves **tests**, the **judge tool**, and **MCP `kiln_rules_scenario`**.

```yaml
scenario: kiln-scenario/1
name: Thassa's Oracle vs Platinum Angel
cr: ["101.2", "104.2b"]
format: commander
players:
  - id: P1
    life: 40
    commander: Tymna the Weaver
    hand: [Thassa's Oracle]
    library: []                      # empty library
    battlefield:
      - { card: Island }
      - { card: Island }
  - id: P2
    life: 40
    battlefield:
      - { card: Platinum Angel }
  - id: P3
  - id: P4
turn: { active: P1, step: precombat_main, number: 5 }
priority: P1
script:                              # choices by option label (resolved to indices at runtime)
  - { player: P1, choose: "Cast Thassa's Oracle" }
  - { pay: auto_first_plan }
  - pass_all                          # all players pass → resolves
  - { player: P1, choose: "Put trigger on stack" }   # if a decision is raised
  - pass_all
expect:
  result: in_progress
  events_include:
    - { type: ability_resolved, source: "Thassa's Oracle" }
  events_exclude:
    - { type: player_won }
```

## Notes
- Unspecified libraries are filled with basic lands, or are empty if `library: []`.
- Card references are by name and resolved through the card DB. Ambiguous names are an error.
- Object attributes include `tapped`, `counters`, `attached_to`, `face_down`, `summoning_sick`, `controller` (if different from the owner), `damage`, `commander: true`.
- `script` choices are matched by **option label**, so tests survive option reordering. Raw indices are allowed for replays.
