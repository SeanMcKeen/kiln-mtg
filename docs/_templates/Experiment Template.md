---
type: experiment
status: draft
tags: [experiment]
updated: {{date}}
---

# {{title}}

## Question
## Hypothesis
## Spec
```yaml
spec: kiln-spec/1
name: …
format: commander
mode: pod
games: 5000
seed: 1
seating: balanced_rotation
decks: []
policies: { default: { kind: compose, chain: [ { kind: playbook, playbook: auto }, { kind: search }, { kind: greedy } ] } }
metrics: { win_rates: true, game_shape: true, mulligans: true }
```
## Results (link the run report)
## Interpretation & caveats
