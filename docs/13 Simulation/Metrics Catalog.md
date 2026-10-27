---
type: design
status: proposed
tags: [simulation, metrics, headline]
updated: 2026-10-27
---

# Metrics Catalog

The user's requirement: **"as much data as possible, though the model or user should be able to set flags for each thing."** Every category below is an independent flag in the [[Experiment Spec]] `metrics:` block. Off means zero cost: the engine's `EventSink` for that category isn't installed ([[Event System]]).

Every rate is reported with a **95% CI**. The default method is Wilson for proportions and bootstrap for means and medians ([[Statistics for Multiplayer Simulation]]).

## `win_rates` *(default: on)*
| Metric | Notes |
|---|---|
| Win rate per deck | Overall, plus vs. the 1/N baseline (25% in a 4-pod) |
| Win rate per deck **by seat** | Seat 1–4. Exposes turn-order advantage |
| Draw rate, capped-game rate | Capped games are excluded from win rate but reported |
| Head-to-head "who won when X lost" matrix | Who's beating whom |
| Elimination order | First out, second out, … per deck |

## `game_shape` *(default: on)*
| Metric | Notes |
|---|---|
| Game length (turns, rounds), distribution | Mean, median, p10, p90 |
| **Win turn** distribution per deck | "How fast does it win?" |
| **Win type** mix | Alt-win (Thoracle, LabMan), combo damage or drain, combat, commander damage, mill or decking, poison, concession, opponents leaving |
| Wins by "combo assembled" vs. "grind" | Uses playbook or line tags when available |
| Turn of first loss per deck | |

## `mulligans` *(default: on)*
| Metric | Notes |
|---|---|
| Keep rate at 7, 6, 5, … | Per deck, under its mulligan policy |
| Win rate by mulligan count | |
| Opening-hand feature rates | Fast mana in opener, tutor in opener, combo piece in opener. Feature tags come from the playbook or Scryfall Tagger tags |

## `per_card` *(default: off; heavy)*
Per card in selected decks, following 17lands-style definitions adapted for multiplayer:
| Metric | Definition |
|---|---|
| Seen rate | % of games the card was drawn or otherwise seen by its owner |
| Cast/played rate | % of seen games in which it was cast or played |
| **GIH WR** (games-in-hand win rate) | Win rate in games where the card was in the opening hand or drawn |
| **OH WR** | Win rate when in the opening hand |
| **IWD** (improvement when drawn) | GIH WR − win rate when not seen. **Confounded** (see caveats) |
| Cast WR | Win rate when cast |
| Turn first cast | Distribution |
| Dead-card rate | Seen but never castable or cast before the game ended |
| Tutor target rate | How often it was fetched by a tutor (for combo pieces) |

> [!warning] Per-card stats are correlational
> Long games see more cards. Losing players draw extra cards from wheels. Tutors bias toward combo pieces. Per-card numbers need large samples and careful reading ([[Validity & Caveats]]). For causal answers, use an **A/B swap** (`mode: ab`).

## `interaction` *(default: off)*
Counterspells cast and their targets, removal targets, stax pieces' "spells prevented" counts, how often win attempts were countered or disrupted, and protection spells used.

## `mana` *(default: off)*
Mana available per turn per deck, mana spent vs. wasted, fast-mana usage, colour screw rate, land drops missed.

## `timing` *(default: off)*
Engine time vs. policy time per seat, decisions per game, games per second. For tuning budgets.

## `decision_stats` *(default: off)*
Decision counts by kind (priority, targets, triggers ordered…), how often each policy component answered (for composed policies), search-AI depth and iterations, and LLM tokens and cost.

## `replays` *(default: interesting, max 50)*
Full replay logs, kept as `none | all | sample:N | interesting`. "Interesting" means:
- the fastest wins
- upsets (the lowest-win-rate deck winning)
- capped games
- the longest games
- games with determinism or engine warnings

## `raw_events` *(default: off; huge)*
The full event stream per game, as Parquet or JSONL, for custom analysis.

## Definitions as implemented (M7)
- **Win turn and loss turn** count the deck's own turns (its Nth turn), not the game's turn number. Game length is reported in game turns and in rounds.
- **Win type**: an effect win while opponents remain is `alt_win`. Otherwise it's how the last opponent left: `life`, `commander_damage`, `decking`, `poison`, `concession` or `effect_loss`.
- **Finishing positions**: index 0 is the first player out; the winner is last. Players still in when an effect win or draw ends the game share the next position.
- **Seen**: the card was in the opening hand (turn 0) or reached its owner's hand later (draws, tutors, returns). Commanders and tokens aren't tracked. **Cast** includes playing a land.
- **IWD** is the win rate in games seen minus the rate in games not seen. Its q-value is Benjamini–Hochberg over the deck's cards with at least `min_samples` games seen.
- **Interaction**: counterspells cast (spells with a counter effect), its own spells countered, and times its spells and abilities targeted a permanent another player controlled.
- **Mana**: lands played and mana units produced per game, and the share of its first five turns with no land played.
- **Timing** uses wall-clock time and is labelled non-deterministic; every other number is a pure function of the game records.

## Related
[[Output Formats & Report Schema]] · [[Statistics for Multiplayer Simulation]] · [[Validity & Caveats]]
