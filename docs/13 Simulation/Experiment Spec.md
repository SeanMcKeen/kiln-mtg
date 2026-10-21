---
type: design
status: proposed
tags: [simulation, spec, schema]
updated: 2026-10-20
---

# Experiment Spec

A declarative, versioned (`mtg-spec/1`) YAML/JSON document. Decision record: [[ADR-0011 Experiment Spec and Report Schema]].

## Example: the headline cEDH pod (US-1)
```yaml
spec: mtg-spec/1
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
  default: { kind: search, budget: { iterations: 400 }, playbook_hints: true }
  kinnan:  { kind: search, budget: { iterations: 400 }, playbook: playbooks/kinnan.rhai }
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

## Related
[[Simulation Harness]] · [[Policy Composition]] · [[Metrics Catalog]]
