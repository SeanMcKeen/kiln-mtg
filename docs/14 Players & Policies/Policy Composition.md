---
type: design
status: proposed
tags: [policies, composition]
updated: 2026-10-27
---

# Policy Composition

The user asked for all four policy kinds, **"using all of the above when the situation calls for it. Or just having them all as an option."** Composition provides both.

## Composition primitives
| Combinator | Meaning | Example |
|---|---|---|
| `Fallback(a, b)` | Ask `a`. If it defers, ask `b` | playbook → search → greedy |
| `ByDecisionKind{…}` | Route by decision type | Mulligan → playbook. OrderTriggers → greedy. Priority → search |
| `ByCondition(pred, a, b)` | Route on a view predicate | "If an opponent's spell is on the stack → search with a bigger budget" |
| `Budgeted(policy, budget)` | Time or iteration limits | search, 400 iterations |
| `AutoPass(policy, rules)` | Skip trivial decisions | "pass when there are no castable instants and nothing targets me" |
| `Guided(search, prior=playbook, rollout=playbook)` | Playbook-guided search | the recommended competent default |
| `Escalate(cheap, expensive, when)` | Use the expensive policy only for "major" decisions | search by default, LLM on win attempts |
| `Recorder(policy)` | Log rationale and decision stats | `decision_stats` metric |

## In the experiment spec
```yaml
policies:
  default:
    kind: compose
    chain:
      - { kind: playbook, playbook: auto }         # if the deck has one
      - { kind: search, budget: { iterations: 400 }, rollout: playbook_or_greedy }
      - { kind: greedy }
    auto_pass: standard
  kinnan:
    kind: escalate
    base: { kind: search, budget: { iterations: 400 } }
    expensive: { kind: llm, model: "<agent's choice>", max_calls_per_game: 5 }
    when: [mulligan, win_attempt, respond_to_win_attempt]
```

## Defaults
- **Fair default:** every deck gets the same policy recipe, `compose[playbook?, search, greedy]`, to avoid asymmetric pilot strength ([[Validity & Caveats]]).
- The report states each seat's policy recipe, and how often each component actually answered.

## As implemented (M8)
- `kiln-policy::Compose` is `Fallback` over a chain: each policy may defer (`Policy::try_decide`), and the last must always answer. Spec: `{ kind: compose, chain: [ … ] }`.
- `{ kind: playbook, playbook: <file>, fallback: <policy> }` (greedy by default) wraps a Rhai playbook ([[B - Playbooks]]).
- Reports list each deck's recipe and counters: `playbook:answered`, `fallback:answered`, `answered:<component>`, `playbook:errors`, `tag:<name>`.
- Not yet: `ByDecisionKind`, `ByCondition`, `Budgeted`, `AutoPass`, `Guided` and `Escalate` (they need search, M9, or LLM policies, M10). A playbook already routes by decision kind through its function names.

## Related
[[Policy Interface]] · [[Experiment Spec]] · [[A - Baseline Policies]] · [[B - Playbooks]] · [[C - LLM Live Decisions]] · [[D - Search AI]]
