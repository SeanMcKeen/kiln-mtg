---
type: design
status: draft
tags: [simulation, validity, statistics]
updated: 2026-10-20
---

# Validity & Caveats

A simulation result is only as good as its weakest link. The report must **surface** these threats, not hide them.

| Threat | What goes wrong | Mitigation |
|---|---|---|
| **Unsupported or unverified cards** | A card silently does nothing, so the deck looks worse (or better) | Coverage gate (FR-S8). The report lists any non-verified cards prominently. |
| **Play quality** | Results measure the *pilot*, not the deck. A random policy makes combo decks look terrible. | Prefer search plus playbooks. Report policy kinds per seat. [[Measuring Play Quality]]. Validate against real data ([[cEDH Ground-Truth Data]]). |
| **Asymmetric pilot strength** | Deck A gets a tuned playbook while B gets greedy | Warn when policy kinds differ across decks. Offer "same policy for all" as the default. |
| **Seat bias** | Turn order strongly affects cEDH outcomes | `balanced_rotation` seating by default. Report by-seat win rates. |
| **Sample size** | A 2-point difference with ±1.2 pp CIs is noise | Always show CIs. Flag comparisons whose CIs overlap. For p = 0.25 and n = 5,000, the 95% half-width is about ±1.2 pp. |
| **Per-card confounding** | Long games see more cards, and wheels and tutors bias "seen" | Label per-card stats as correlational. Recommend A/B swaps for causal questions. |
| **Multiple comparisons** | 100 cards × metrics will produce false "significant" outliers | Report adjusted intervals or a false-discovery rate (Benjamini–Hochberg) for per-card rankings. |
| **Meta / decklist knowledge** | `open_decklists: true` lets the search AI know what's coming | Report the flag. Optionally run both settings. |
| **Engine bugs** | Wrong rules mean wrong results | CR-cited tests, golden replays, fuzzing. Report the engine version. |
| **Capped games** | Policies stall and loop | Report the capped rate. A high rate is a warning. |

## Related
[[Statistics for Multiplayer Simulation]] · [[Measuring Play Quality]] · [[Metrics Catalog]]
