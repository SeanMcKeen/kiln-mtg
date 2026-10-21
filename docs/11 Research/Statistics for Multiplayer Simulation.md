---
type: research
status: draft
tags: [research, statistics, simulation]
updated: 2026-10-20
---

# Statistics for Multiplayer Simulation

## Confidence intervals for win rates
Use the **Wilson score interval** for proportions. It behaves well near 0 and 1 and at small n. For means and medians (win turn), use the **bootstrap** (percentile or BCa) with a seeded RNG.

**Sample size intuition (4-pod, p ≈ 0.25, 95% CI half-width ≈ 1.96·√(p(1−p)/n)):**
| Games | ± half-width |
|---|---|
| 1,000 | ±2.7 pp |
| 2,000 | ±1.9 pp |
| 5,000 | ±1.2 pp |
| 10,000 | ±0.85 pp |
| 20,000 | ±0.6 pp |

Games aren't fully independent within a seed scheme, but with independent seeds per game this approximation holds.

## Seat (turn-order) effects: huge in cEDH
Community data suggests a **strong first-seat advantage**:
- an EDHREC article cites roughly seat 1 ≈ 35–40%, seat 2 ≈ 25–30%, seat 3 ≈ 20%, seat 4 ≈ 10–15%
- older self-reported Reddit data shows about 32/29/21/16%, with acknowledged reporting bias

Implications:
- **`balanced_rotation` seating** is the default. Each deck sits in each seat equally often. Use the 4 cyclic rotations at minimum, or all 24 permutations to also balance *who sits next to whom*.
- Report **by-seat win rates**. A credible sim should *reproduce* a strong seat-1 advantage, which is a useful validation signal ([[Measuring Play Quality]]).

## Comparing decks and variants
- **Paired A/B with common random numbers:** the same seeds for variant A and variant B, with only one deck changed. The difference estimator has much lower variance than independent runs. Use a paired bootstrap or a McNemar-style test on per-seed outcomes.
- **Multiplayer ratings:** Plackett–Luce or TrueSkill over finishing orders, rather than just wins, for policy ladders.
- **Draws and capped games:** report them separately. Exclude capped games from win-rate denominators, but report their rate.

## Multiple comparisons
Per-card rankings test many hypotheses. Control the false-discovery rate (**Benjamini–Hochberg**) or show shrunken (empirical-Bayes) estimates. Always print n per card.

## Sequential stopping (optional)
Stopping when the CIs look good inflates error if it isn't pre-registered. If early stopping is offered, use a pre-declared rule (a target CI width) or an always-valid sequential method.

## Sources
- EDHREC, "Does cEDH Have a Seat Order Problem?": https://edhrec.com/articles/does-cedh-have-a-seat-order-problem
- Reddit seat-data discussion (self-reported, biased): https://lr.ggtyler.dev/r/magicTCG/comments/wjcjn6/crazy_cedh_tournament_in_los_angeles_announced/ijh2sc3/?context=3
- Standard references for the Wilson interval, bootstrap, Benjamini–Hochberg, Plackett–Luce and TrueSkill (textbook methods)
