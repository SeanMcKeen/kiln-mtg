---
type: option
status: draft
tags: [card-definitions, option, leaning]
updated: 2026-10-20
---

# Option D: Hybrid, Parser-First with DSL Overrides (current leaning)

## What
One **typed IR** in Rust, with three front-ends that all compile to it:
1. **Oracle parser**, the default. A compositional grammar over templated Oracle text. It handles the regular majority.
2. **DSL override files**, for cards the parser can't do or gets wrong. They're human- or AI-authored, reviewed, and checked in.
3. **Rust escape hatch** for the truly unique (Shahrazad, Karn Liberated, Mindslaver, and so on).

A per-card **provenance** field records which front-end produced the IR. A **coverage status** records whether it's verified.

```
Oracle text ──► parser ──┐
DSL override ────────────┼──► Card IR ──► IR→text renderer ──► diff vs Oracle (lint)
Rust escape fn ──────────┘         │
                                   └──► scenario tests ──► status = verified
```

## Why this is the leaning
- The corpus is regular enough for a parser to carry most of the load ([[Oracle Text Corpus Analysis]]).
- The long tail is real (89% of lines are unique), so overrides must be first-class, not hacks.
- **AI agents** can be part of the pipeline. One prompt: "given this Oracle text and this unparsed span, write the DSL override", then run the generated scenario test.
- Precedent: Arena (parser + hand rules) and Forge (DSL + code) together cover this hybrid's territory.

## Key design questions for the spike
- The IR shape: the effect algebra, the filter algebra, the cost model, and how much of the rules procedures leak into the IR.
- DSL syntax: RON/YAML vs. a custom S-expression or "templated English" syntax.
- How the renderer reconstructs Oracle text well enough to be a useful lint.
