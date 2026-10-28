---
type: design
status: proposed
tags: [simulation, spec, schema]
updated: 2026-10-27
---

# Experiment Spec

A declarative, versioned (`kiln-spec/1`) YAML/JSON document. Decision record: [[ADR-0011 Experiment Spec and Report Schema]].

## Example: the headline cEDH pod (US-1)
```yaml
spec: kiln-spec/1
name: kinnan-vs-meta-oct26
format: commander            # rules variant + deck validation
mode: pod
games: 5000
seed: 20261020
seating: balanced_rotation   # fixed | random | balanced_rotation (each deck in each seat equally)
open_decklists: true         # policies may assume opponents' lists are known (for determinization)

decks:
  - id: kinnan
    source: { file: decks/kinnan.txt }
  - id: blue_farm
    source: { text: "1 Tymna the Weaver\n1 Kraum, Ludevic's Opus\n..." }
  - id: najeela
    source: { moxfield_export: decks/najeela-moxfield.txt }
  - id: thrasios_tymna
    source: { file: decks/thrasios-tymna.txt }

policies:                    # per deck (follows the deck across seats); can also be per seat
  default: { kind: search, budget: { iterations: 32, rollout_depth: 100 } }
  kinnan:  { kind: search, guide: { kind: playbook, playbook: playbooks/kinnan.rhai } }
  najeela: { kind: playbook, playbook: playbooks/najeela.rhai, fallback: { kind: greedy } }

limits:
  max_turns: 30
  max_decisions: 20000
  max_wall_ms: 60000
  coverage: require_verified   # require_verified | warn | allow

metrics:                     # every category independently togglable (see Metrics Catalog)
  win_rates: true
  game_shape: true
  mulligans: true
  per_card: { enabled: true, decks: [kinnan], min_samples: 200 }
  interaction: true
  mana: false
  timing: false
  decision_stats: false

outputs:
  dir: runs/kinnan-vs-meta-oct26
  summary: json                # json | markdown
  tables: parquet              # parquet | jsonl | sqlite | none
  replays: { keep: interesting, max: 50 }   # none | all | sample:N | interesting
```

## Other modes (short)
```yaml
mode: goldfish
decks: [{ id: kinnan, source: { file: decks/kinnan.txt } }]
goldfish: { opponents: 3, opponents_behavior: passive, life: 40 }
```
```yaml
mode: ab
variants:
  remora: { base: kinnan, swap: { out: ["Rhystic Study"], in: ["Mystic Remora"] } }
paired: true                 # same seeds per pair → common random numbers
```

## Validation rules
- Every deck passes format legality. Every card meets the `limits.coverage` policy.
- `balanced_rotation` requires `games` divisible by the number of rotations (4 for 4 decks with cyclic rotation, 24 for all permutations). Otherwise it's rounded up, with a warning.
- Unknown keys are an **error**, with a "did you mean" suggestion (agent-friendly).
- The resolved spec, with defaults filled in, is echoed back and stored in the run directory, and its hash goes into the report.

## As implemented (M7)
- Types live in `kiln-protocol` (`crates/kiln-protocol/src/spec.rs`); the JSON Schema is `crates/kiln-protocol/schemas/kiln-spec-1.schema.json`. Examples: `examples/specs/`.
- Defaults: `format: commander`, `mode: pod`, `games: 1000`, `seed: 0`, `seating: balanced_rotation`, `limits: { max_turns: 100, max_decisions: 200000, coverage: require_verified }`, metrics `win_rates`, `game_shape` and `mulligans` on, `outputs: { dir: runs/<name>, summary: json, tables: jsonl, replays: { keep: interesting, max: 50 } }`.
- Policy kinds now: `random { pass }`, `greedy` (the default), `passive`, `playbook { playbook, fallback }` and `compose { chain }` (M8), and `search { budget: { iterations, rollout_depth, ms }, guide }` (M9; defaults 32 iterations, depth 100, guide greedy). `llm { model, ask: key|all, max_calls, effort, fallback }` (M10) asks Claude through the Anthropic API at key decisions (or all) and hands the rest to `fallback` (greedy by default); it costs money per request and isn't reproducible, and the report says so. A search `guide` must always answer and not search; a playbook guide makes it playbook-guided search ([[D - Search AI]]). Policies follow the deck; a variant uses its own entry, else its base deck's, else `default`.
- Deck sources: `file` (relative to the spec), `text`, `moxfield_export` (pasted text). URLs arrive with M8.
- Seat 1 always plays first; seating moves the decks. `all_permutations` cycles through all N! orders.
- Goldfish opponents play decks of Islands and never act. `opening_hands` deals and mulligans each deck in a 4-player pod with three passive opponents (so the first mulligan is free, CR 103.5c) and stops as turn 1 begins; `opening_hands.features` names card groups to look for in kept hands.
- `max_wall_ms` makes results machine-dependent and the report warns. Tables: `jsonl` or `csv` (Parquet: Q38).

## Related
[[Simulation Harness]] · [[Policy Composition]] · [[Metrics Catalog]]
