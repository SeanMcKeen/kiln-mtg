---
type: option
status: draft
tags: [card-definitions, option]
updated: 2026-10-20
---

# Option A: Declarative DSL + Escape Hatch

## What
Each card is a data file (TOML/YAML/RON or a custom syntax) that composes IR primitives: abilities, costs, triggers, effects, filters, conditions. Cards the DSL can't express use a named Rust function.

**Prior art:** Forge's card scripts. Example keys: `A:SP$ DealDamage | ValidTgts$ … | NumDmg$ …`, `T:` triggers, `S:` statics, `R:` replacements, `SVar` variables, plus AI hints ([[Prior Art - Forge]]).

## Sketch
```yaml
name: Swords to Plowshares
abilities:
  - spell:
      targets: [{ filter: "creature" }]
      effects:
        - exile: { target: 0 }
        - gain_life: { player: { controller_of_target: 0 }, amount: { power_of_target_lki: 0 } }
```

## Pros
- Explicit and reviewable. A diff shows exactly what a card does.
- Fast to load (pre-compiled to IR), with no parse ambiguity.
- **LLMs can author it well** from Oracle text, and tests verify it.
- Forge proves the approach scales to about 30k cards, but with a large community.

## Cons
- ~32k files to author and review, even with AI help.
- DSL design drift: every new mechanic needs DSL extensions.
- Errata require manual edits, and Oracle text and DSL can silently diverge.

## Cost estimate
High authoring volume, with moderate per-card effort and low tooling effort.
