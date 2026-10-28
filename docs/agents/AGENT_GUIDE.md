---
type: guide
status: active
tags: [agents, guide, mcp, simulation]
updated: 2026-10-27
---

# Kiln Agent Guide

Kiln is a rules-exact Magic: The Gathering engine with a simulation lab. It's built for Commander, cEDH 4-player pods in particular. You give it decklists and an experiment spec; it plays thousands of games and reports results **with 95% confidence intervals and validity warnings**. This guide is for AI agents with no prior knowledge of Kiln ([[Agent-Facing Docs Plan]], FR-I5).

## Setup

```bash
cargo build --release                      # the CLI binary is target/release/kiln
./target/release/kiln data fetch           # Scryfall bulk data (cached; never committed)
./target/release/kiln data build
```

**MCP (recommended for agents).** Register the server with your client. Claude Code:

```bash
claude mcp add kiln -- /path/to/kiln mcp serve --workdir /path/to/workspace
```

Or use `examples/mcp.json` as a template. The server loads the card library on startup (a few seconds), then serves nine tools over stdio. Imported decks, playbooks and runs live under `--workdir`.

**CLI.** Every MCP tool has a CLI equivalent (`kiln deck check`, `kiln sim run|status|report|compare`, `kiln game inspect|replay`, `kiln playbook check`). Each takes `--json`.

## The headline task (US-1), step by step

> "Evaluate my Kinnan list against Blue Farm, Najeela and Thrasios/Tymna over 5,000 games."

1. **Import each deck**: `kiln_deck_import { source: <pasted list or path>, name: "kinnan" }`. Check `legal` and the coverage lists. Only **verified** cards run by default. `parsed_not_verified` cards need `limits: { coverage: warn }` (the report then warns), and `unsupported` cards can't run at all; tell the user which ones. Keep the returned `file` path.
2. **(Optional) Write a playbook per deck**, see below, and check each with `kiln_playbook_validate`.
3. **Write the spec** and start it with `kiln_sim_run { spec_text }`:

```yaml
spec: kiln-spec/1
name: kinnan-vs-meta
games: 5000                  # ~±1.2 pp intervals at a 25% win rate
seed: 20261027
seating: balanced_rotation   # each deck in each seat equally often (the default)
decks:
  - { id: kinnan,    source: { file: decks/kinnan.txt } }
  - { id: blue_farm, source: { file: decks/blue-farm.txt } }
  - { id: najeela,   source: { file: decks/najeela.txt } }
  - { id: thrasios,  source: { file: decks/thrasios.txt } }
policies:
  default: { kind: greedy }
  kinnan:  { kind: playbook, playbook: playbooks/kinnan.rhai }   # optional
```

4. **Poll** `kiln_sim_status { run_id: "kinnan-vs-meta" }` until `status` is `complete`. 5,000 greedy games take about 30 s on 2 threads.
5. **Read** `kiln_sim_report { run_id }`: the concise form has win rates with intervals, by-seat rates, median win turns, win types, keep rates, warnings and `next` hints. Ask for `format: detailed` with `sections: [per_card]` for card-level data.
6. **Answer with the intervals.** See "Reading results".

## Other experiments

| Question | Spec | Example |
|---|---|---|
| How fast does my deck win unopposed? (US-2) | `mode: goldfish`, `goldfish: { opponents: 3 }` | `examples/specs/najeela-goldfish.yaml` |
| How often are my hands keepable? (US-4) | `mode: opening_hands`, `opening_hands: { features: { fast_mana: [Sol Ring, …] } }` | `examples/specs/kinnan-opening-hands.yaml` |
| Does swapping a card help? | `mode: ab`, `variants: { v: { base: deck, swap: { out: [A], in: [B] } } }` | `examples/specs/m6-ab.yaml` |
| A cEDH pod | `mode: pod` (default) | `examples/specs/m6-pod.yaml`, `us1-kinnan-pod.yaml` |

The full spec is `kiln-spec/1` ([[Experiment Spec]]; JSON Schema: `crates/kiln-protocol/schemas/kiln-spec-1.schema.json`). Unknown keys are errors that name the closest valid key.

## Reading results

- **Every rate has a 95% interval.** Two decks whose intervals overlap can't be ranked; say so. About 5,000 games give ±1.2 pp at 25%; 1,000 give ±2.7 pp.
- **Win turn** is the deck's own turn ("a turn-4 win"), not the game's turn number.
- **Seat bias**: balanced rotation spreads it evenly. By-seat rates show it.
- **Pilots matter**: random and greedy are baselines, not expert play. The report says which policy each deck used and warns. Results measure *this* pilot. Real cEDH shows a strong seat-1 advantage; baseline pilots don't (open question Q39).
- **Capped games** (hitting `max_turns` or `max_decisions`) are left out of win rates; the report warns when there are many.
- **Per-card statistics are correlational.** Long games see more cards, so nearly every card shows a positive IWD. For a causal answer, run `mode: ab`.
- **Independent runs** (`kiln_sim_compare`) differ for every reason that changed between them. When comparing pilots, change one deck's policy at a time (`examples/specs/m8-kinnan-playbook-only.yaml`). Keep the same `seed` and `seating`: the games then pair up, and `kiln_sim_compare` reports a tighter `paired_ci95` per deck.
- **Goldfish limits count game turns**: with 3 opponents, `max_turns: 60` is 15 of the deck's own turns. A deck that kills with combat may need 100 or more.
- **Polling**: `kiln_sim_status` says when to poll again; don't poll every second.

## Writing playbooks

A playbook is a [Rhai](https://rhai.rs) script that pilots one deck ([[B - Playbooks]]). Define functions named after decision kinds, each taking `(view, decision)`:

```rhai
fn mulligan(view, d) {
    let lands = view.hand.filter(|c| c.land).len();
    if lands >= 2 && lands <= 4 { tag("keep"); return "Keep"; }
    tag("mulligan");
    "Mulligan"
}

fn priority(view, d) {
    let land = find_option(d.options, "Play ");
    if land >= 0 { tag("land drop"); return land; }
    ()   // defer to the fallback policy
}
```

- **Decision kinds**: `mulligan`, `bottom_card`, `priority`, `choose_target`, `declare_attacker`, `declare_blocker`, `optional`, `choose_card`, `choose_mode`, `vote`, … or a catch-all `decide`.
- **`decision`**: `kind`, `options` (labels such as `"Cast Sol Ring"`, `"Play Island"`, `"Keep"`, `"Najeela, the Blade-Blossom attacks P3"`, `"Target P2"`, `"Yes"`), `count`.
- **`view`** (the deciding player's redacted view; nothing hidden): `me`, `turn`, `my_turn`, `step`, `players[i]` (`seat`, `life`, `hand_count`, `library_count`, `in_game`), `hand`, `battlefield`, `stack`, `graveyard`, `graveyards`, `command`. Objects have `name`, `types`, `mana_value`, `land`, `creature`, `controller`, `owner`, `tapped`, `power`, `toughness`, `attacking`, `token`, `commander`.
- **Return** an option index, an option label (exact or a unique substring), or `()` to defer. Invalid answers, errors and runaway loops defer too, and are counted.
- **Helpers**: `find_option(options, text)` (-1 if absent), `has_option(options, text)`, `names(cards)`, `tag(name)` (counts why you chose; reports show tag counts).
- **Pitfall**: copy view fields into locals before closures (`let me = view.me; view.battlefield.filter(|c| c.controller == me)`). A closure that captures `view` fails with "data race".
- **Validate** with `kiln_playbook_validate`. It reports compile errors with line:column, then rule counts, the playbook's share of decisions and runtime errors from goldfish games. Then use it in a spec as `{ kind: playbook, playbook: <file>, fallback: { kind: greedy } }`, or compose: `{ kind: compose, chain: [ {kind: playbook, …}, {kind: greedy} ] }`.
- Examples: `examples/playbooks/` (one per test deck, plus `generic-cedh.rhai`).

## Rules questions

`kiln_rules_scenario` runs a board you describe ([[Scenario File Format]]): each player's zones, an optional starting turn and step, a script of choices by label, and expectations. It returns the event log with card names and whether the expectations held. `tests/scenarios-real/` has hundreds of examples.

## Pitfalls checklist

- Report intervals, not just point estimates. Flag overlaps.
- List unverified or unsupported cards to the user before running; never hide them.
- Use `mode: ab` for "should I play card X?" questions.
- Don't over-read per-card IWD.
- Say which pilots played. Baseline pilots understate combo decks.
- A run with the same `name` resumes; change the name for a new experiment.

## Related
[[MCP Server Tool Catalog]] · [[Experiment Spec]] · [[Metrics Catalog]] · [[Validity & Caveats]] · [[B - Playbooks]]
