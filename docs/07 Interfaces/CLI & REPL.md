---
type: design
status: draft
tags: [interfaces, cli]
updated: 2026-10-20
---

# CLI & REPL

A single binary, `mtg`, with subcommands. Every command supports `--json` for machine output. Human output is the default on a TTY.

## Command sketch
```
mtg data fetch|build|coverage|diff            # see Ingestion Pipeline
mtg deck check <file|url> [--format commander] [--json]
mtg sim run <spec.yaml> [--threads N] [--json] [--detach]
mtg sim status <run-id> [--json]
mtg sim report <run-id> [--sections win_rates,per_card] [--format concise|detailed|markdown] [--json]
mtg sim compare <run-a> <run-b> [--metric win_rate]
mtg game replay <replay-file> [--to-turn 5] [--annotate]
mtg game inspect <run-id> <game-index>
mtg judge <scenario.yaml> [--step] [--explain]    # interactive REPL with --step
mtg playbook check <deck> <playbook>
mtg mcp serve [--stdio | --http :8080]            # MCP server ([[MCP Server Tool Catalog]])
```

## Judge REPL (interactive)
```
mtg judge scenarios/thoracle-vs-angel.yaml --step
[Turn 4 · P1 main 1 · P1 priority]  options:
  0) Cast Thassa's Oracle ({U}{U})        1) Pass priority
> 0
Thassa's Oracle resolves → enters → trigger (CR 603.2) …
> why 1 cannot win?
  P2 controls Platinum Angel: "your opponents can't win the game" (CR 101.2: "can't" overrides)
```
See [[Judge Tool UX]].

## Exit codes
0 ok · 2 invalid input (with actionable JSON error) · 3 unsupported cards (coverage gate) · 4 run failed · 5 determinism divergence (replay).

## Related
[[Library API]] · [[Judge Tool UX]] · [[AI-Agent Integration Research]]
