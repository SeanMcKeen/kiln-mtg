---
type: design
status: proposed
tags: [simulation, report, schema, agents]
updated: 2026-10-20
---

# Output Formats & Report Schema

## Run directory
```
runs/<run-id>/
  spec.resolved.yaml        # with defaults filled in; hash recorded
  report.json               # mtg-report/1 (full)
  report.md                 # human summary
  games.parquet             # one row per game (or games.jsonl)
  cards.parquet             # per-card aggregates (if per_card)
  events/…                  # raw events (if raw_events)
  replays/<game-index>.json # mtg-replay/1 (per replays policy)
  progress.jsonl            # checkpoint log
```

## Concise report (agent-facing, ≈ ≤ 2k tokens)
```json
{
  "report": "mtg-report/1", "run_id": "kinnan-vs-meta-oct26", "games": 5000, "capped": 31,
  "versions": { "engine": "0.7.0", "card_db": "scryfall-2026-10-20#ab12", "cr": "2026-06-19", "spec_hash": "…" },
  "warnings": ["2 cards in 'najeela' are 'parsed' but not 'verified' (run with coverage=warn)"],
  "win_rates": [
    { "deck": "kinnan",         "win_rate": 0.291, "ci95": [0.278, 0.304], "by_seat": [0.34, 0.30, 0.27, 0.25] },
    { "deck": "blue_farm",      "win_rate": 0.262, "ci95": [0.250, 0.275] },
    { "deck": "najeela",        "win_rate": 0.231, "ci95": [0.219, 0.243] },
    { "deck": "thrasios_tymna", "win_rate": 0.210, "ci95": [0.199, 0.222] }
  ],
  "draw_rate": 0.006,
  "game_shape": { "median_win_turn": { "kinnan": 5, "blue_farm": 4, "najeela": 6, "thrasios_tymna": 6 },
                  "win_types": { "kinnan": { "alt_win": 0.12, "infinite_mana_drain": 0.71, "combat": 0.17 } } },
  "mulligans": { "kinnan": { "keep_at_7": 0.78, "win_rate_by_mulls": [0.30, 0.27, 0.19] } },
  "notable": ["Seat 1 advantage: +9 pp vs seat 4 (expected in cEDH)",
              "Kinnan loses 64% of games where Najeela resolves Silence on Kinnan's turn"],
  "next": ["mtg_sim_report sections=[per_card] deck=kinnan", "mtg_game_inspect query='kinnan lost before turn 4'"]
}
```
*(Numbers are illustrative only.)*

The `next` hints follow the agent-tool guidance to steer follow-up calls ([[AI-Agent Integration Research]]).

## Detailed report
The full `report.json` holds every enabled metric section, paginated when served over MCP (`page`, `page_size`, `filter`), with artifact paths for the bulk tables.

## Tables
- **games:** `game_index, seed, seating[], winner, win_type, turns, capped, mulligans[], first_out, …, policy_time_ms[]`
- **cards:** `deck, oracle_id, name, seen, cast, gih_wr, gih_ci_lo, gih_ci_hi, oh_wr, iwd, dead_rate, tutor_rate, …`

## Schema governance
JSON Schemas are generated from `mtg-protocol` (`schemars`). Versions bump on breaking changes. Readers accept older minor versions.

## Related
[[Metrics Catalog]] · [[MCP Server Tool Catalog]] · [[ADR-0011 Experiment Spec and Report Schema]]
