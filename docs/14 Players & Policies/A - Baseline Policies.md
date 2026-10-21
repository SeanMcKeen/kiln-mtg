---
type: design
status: proposed
tags: [policies, baseline]
updated: 2026-10-20
---

# A: Baseline Policies

**Purpose:** required infrastructure. They make sims work out of the box, act as fuzzers, set lower bounds for play quality, and serve as fallbacks inside compositions.

| Policy | Behaviour | Speed | Use |
|---|---|---|---|
| `random` | Uniform over legal options, with configurable pass bias (otherwise it passes or acts absurdly) | Fastest | Fuzzing ([[Fuzzing via Random Play]]), engine stress, a lower bound |
| `greedy` | Simple heuristics: always play a land; cast the highest-value castable spell (MV, then card-type priority); attack when "safe" by a simple board evaluation; never counter; random but legal targets with a preference for opponents' best permanents | Very fast | Default fallback, goldfish speed tests |
| `passive` | Always passes, keeps every hand, no blocks | Fastest | Goldfish opponents |
| `scripted` | Follows a fixed choice list (from a replay or a test) | Fastest | Tests, reproducing bugs |

> [!warning] Honest limitation
> Baselines don't hold up interaction, don't recognize lethal they could assemble, and don't respond on the stack. **cEDH win rates measured with baselines say almost nothing about deck strength.** The report flags runs where any seat uses only baselines ([[Validity & Caveats]]).

## Related
[[Policy Composition]] · [[D - Search AI]]
