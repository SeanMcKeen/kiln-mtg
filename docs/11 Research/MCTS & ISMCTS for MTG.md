---
type: research
status: draft
tags: [research, ai, search]
updated: 2026-10-20
---

# MCTS & ISMCTS for MTG

## Key papers
- **Ward & Cowling (2009):** "Monte Carlo search applied to card selection in Magic: The Gathering". The precursor work.
- **Cowling, Ward & Powley (2012):** "Ensemble Determinization in Monte Carlo Tree Search for the Imperfect Information Card Game Magic: The Gathering", IEEE Trans. CIAIG. Findings:
  - it uses **determinization** (sampling hidden info) plus MCTS
  - an **ensemble of determinized binary trees**, with action values summed
  - **move pruning with domain knowledge** and a **binary tree representation** tame the branching factor
  - it **consistently beat an expert rule-based player**

  https://eprints.whiterose.ac.uk/75050/
- **Cowling, Powley & Whitehouse (2012):** "Information Set Monte Carlo Tree Search", IEEE Trans. CIAIG. It searches **trees of information sets** instead of determinized states, which addresses **strategy fusion**. https://eprints.whiterose.ac.uk/75048/
- **Whitehouse (2014), PhD thesis:** ISMCTS outperforms Perfect Information Monte Carlo (PIMC) and solves strategy fusion and non-locality. https://etheses.whiterose.ac.uk/8117/

## Design takeaways for [[D - Search AI]]
1. Start with **SO-ISMCTS** (single observer), with the engine's determinization service.
2. **Domain-knowledge pruning is essential.** Our decision factoring and playbook priors provide it.
3. A **binary or factored action representation** maps onto our factored decisions ([[Decision & Action Model]] R1).
4. **Rollout policy quality matters more than iteration count.** Use greedy or playbook rollouts.
5. Multiplayer needs vector-valued backups (max^n or paranoid variants). Evaluate both.

## Sources
- White Rose eprints: 75050 (ensemble determinization in MTG), 75048 (ISMCTS)
- Whitehouse thesis: etheses.whiterose.ac.uk/8117
- MCTS survey (strategy fusion discussion): https://arxiv.org/pdf/2103.04931
