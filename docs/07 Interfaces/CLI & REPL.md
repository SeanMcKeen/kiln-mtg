---
type: design
status: draft
tags: [interfaces, cli]
updated: 2026-10-27
---

# CLI & REPL

A single binary, `kiln`, with subcommands. Every command supports `--json` for machine output. Human output is the default on a TTY.

## Command sketch
```
kiln data fetch|build|coverage|diff            # see Ingestion Pipeline
kiln deck check <file|url> [--format commander] [--json]
kiln sim run <spec.yaml> [--threads N] [--json] [--detach]
kiln sim status <run-id> [--json]
kiln sim report <run-id> [--sections win_rates,per_card] [--format concise|detailed|markdown] [--json]
kiln sim compare <run-a> <run-b> [--metric win_rate]
kiln game replay <replay-file> [--to-turn 5] [--annotate]
kiln game inspect <run-id> <game-index>
kiln judge <scenario.yaml> [--step] [--explain]    # interactive REPL with --step
kiln playbook check <deck> <playbook>
kiln mcp serve [--stdio | --http :8080]            # MCP server ([[MCP Server Tool Catalog]])
```

## Implemented (M7)
- `kiln sim run <spec> [--threads N] [--dir D] [--json]`: runs or resumes, writes the run directory, prints the markdown summary (or the concise report with `--json`).
- `kiln sim status <dir>`, `kiln sim report <dir> [--format concise|detailed|markdown]`, `kiln sim compare <dir-a> <dir-b>` (per-deck differences, a two-proportion test and an interval-overlap flag; independent runs, so causal card questions belong in `mode: ab`).
- `kiln game replay <file>`: re-runs a `kiln-replay/1` file and checks it reproduces exactly (exit 5 if not). `kiln game inspect <dir> <index> [--config C] [--events N]`: plays a game of a run again with the event log and prints each seat's fate and the last events with card names.
- Not yet: `--detach` (with M8's MCP async pattern), `--to-turn` and `--annotate` for replays, the judge REPL (M10), playbooks (M8), MCP (M8).

## Judge REPL (interactive)
```
kiln judge scenarios/thoracle-vs-angel.yaml --step
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
