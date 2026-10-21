---
type: design
status: draft
tags: [testing, cr]
updated: 2026-10-20
---

# CR-Cited Scenario Tests

Each test is a [[Scenario File Format]] file with:
- `cr: [603.3b, 101.4]`: the rules exercised. CI validates that these exist in the current CR.
- `cards`: the cards involved, which feeds card verification.
- the setup state, a scripted sequence of choices, and expectations on events, final state, and offered options.

## Expectations supported
- `expect_options`: the decision offered at a point contains or excludes specific options. This is how **"illegal actions are impossible"** gets tested.
- `expect_events`: an ordered or unordered subsequence of events.
- `expect_state`: life totals, zones, counters, characteristics (post-layers).
- `expect_result`: win, loss, draw, with the reason.

## Coverage report
`mtg testkit coverage` produces:
- the rules with at least one test vs. all rules in owned sections (by subsystem)
- cards `verified` vs. in the pool

The output feeds `Rules Coverage.base` notes and the roadmap.

## Seed corpus
The [[Tricky Interactions Catalog]] entries T01–T62, plus the "Test scenarios" sections of every subsystem spec.
