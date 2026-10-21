---
type: architecture
status: proposed
tags: [architecture, determinism, replay, reproducibility]
updated: 2026-10-20
---

# Determinism & Replay

Decision record: [[ADR-0005 Determinism and Replay]].

## The determinism contract
```
game_result = f(card_db_version, engine_version, rules_config, decklists, seating, game_seed, choice_log)
```
- **`game_seed`** seeds a ChaCha-based RNG (e.g. `rand_chacha::ChaCha8Rng`) owned by `GameState`. Every random event draws from it in a defined order: shuffles (CR 701.24), coin flips (CR 705), die rolls (CR 706), random discard, random starting player.
- **Policies have their own RNG streams**, derived from `(experiment_seed, game_index, seat)`. Policy randomness then never perturbs game randomness, so swapping one seat's policy doesn't change the shuffles the other seats see. That property is essential for paired A/B experiments, because it gives common random numbers.
- **Per-game seeds** are derived from the experiment seed and the game index with a hash-based splitter. They never come from "next value of a shared RNG". That way parallel scheduling can't change results ([[Batch Runner & Parallelism]]).

## Banned sources of non-determinism in game logic
- `HashMap`/`HashSet` iteration order. Use `BTreeMap`, `IndexMap`, or sorted vectors.
- Wall-clock time. Time budgets in search AI break determinism, so search has an **iteration-budget mode** for reproducible runs ([[D - Search AI]]).
- Floating point in rules logic. Fine in metrics and policies, but kept out of the engine.
- Thread-local state or globals.

## Replay log format (sketch)
```json
{
  "format": "mtg-replay/1",
  "engine": "0.3.0", "card_db": "scryfall-2026-10-20#ab12…", "cr": "2026-06-19",
  "rules": { "variant": "commander", "players": 4 },
  "decks": ["deck-hash-1", "deck-hash-2", "deck-hash-3", "deck-hash-4"],
  "seating": [2, 0, 3, 1],
  "game_seed": "0x5eed…",
  "choices": [[17, 0], [18, 2], [19, 0], …],     // (decision_id, option_index)
  "final_hash": "…", "result": { "winner": 2, "turn": 6, "reason": "thassas_oracle" }
}
```
- Replays are compact: a few KB per game. Decisions are re-derived during replay. Any mismatch (a decision ID or option count differs) is a **determinism bug**, and the replay runner reports the first divergence.
- Annotated replays add the event stream and policy rationale (from LLM or search policies) for human/agent inspection ([[Output Formats & Report Schema]]).

## Versioning
Replays are only guaranteed to reproduce under the same engine and card DB versions. Running a golden replay against a newer version is a **regression test** ([[Golden Replays]]). A divergence is either a bug or an intentional rules fix that needs a re-bless.

## Related
[[Reproducibility]] · [[Randomness]] · [[Golden Replays]]
