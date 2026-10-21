---
type: subsystem
status: draft
cr: ["116", "601", "602", "606", "607"]
impl_status: not-started
milestone: M2
tags: [subsystem, casting, abilities]
updated: 2026-10-20
---

# Casting & Activating

## Purpose
This subsystem runs the multi-step procedures for casting spells (CR 601.2a–i) and activating abilities (CR 602), which are also used for loyalty abilities (CR 606). It also handles special actions (CR 116).

## CR sections
116, 601, 602, 606, 607 (linked abilities).

## Responsibilities
Casting follows the CR's order exactly, as a resumable state machine ([[Game State Model]] continuations). Each step can raise decisions:
1. Move the card to the stack (601.2a). The legality of *starting* depends on casting permissions: zone, timing, "may cast from graveyard" (flashback, escape via Underworld Breach), "can't cast" effects.
2. Announce mode, splice, alternative and additional costs, X, hybrid/Phyrexian choices (601.2b).
3. Choose targets (601.2c).
4. Choose division (601.2d).
5. Legality check (601.2e). Because of [[Design Principles]] §1, the engine pre-filters, so this check should never fail. The check still runs as an assertion.
6. Determine the total cost: additional costs, cost increases and reductions, commander tax (601.2f).
7. Activate mana abilities (601.2g).
8. Pay costs in CR order (601.2h).
9. The spell becomes cast, and cast triggers fire (601.2i).

## Decisions raised
`ChooseModes`, `ChooseX`, `YesNo` (optional additional costs such as kicker), `ChooseOption` (alternative cost: Force of Will pitch, Fierce Guardianship free cast, evoke…), `ChooseTargets`, `DivideDamage`, `PayCost` / `ActivateManaAbility`, `ChooseObjects` (cards to exile or discard as costs).

## Legality pre-computation (the hard part)
To offer only legal casts, the engine must know before offering "cast X" that **some** complete legal sequence exists. That means:
- legal targets exist for every required target slot
- the total cost is payable, given available mana sources, *including mana abilities that need their own decisions*

**Approach:** a fast conservative check first ("enough potential mana of the right colors, targets exist"). Then a precise check runs lazily when the player selects the action. If the precise check fails mid-procedure, the engine backs out per CR 601.2e/733 (rewind), and **that option is removed from the menu with a reason** for the judge explanation. Open: making the rewind invisible to policies. See [[12 Open Questions]].

## Edge cases (cEDH-heavy)
- **Lion's Eye Diamond:** a mana ability with a discard-hand cost, activated *while casting* (CR 605.3a), which is the Underworld Breach line.
- **Force of Will / Force of Negation / Fierce Guardianship / Deflecting Swat / Pact of Negation:** alternative costs with conditions.
- **Underworld Breach:** grants escape to nonland cards in the graveyard. Casting from the graveyard with an exile-cards cost.
- **Bolas's Citadel:** play the top of the library, paying life instead of mana.
- **Isochron Scepter:** copy and cast without paying the mana cost; the cast-with-alternative-cost rules apply.
- **Commander tax:** the CR 903.8 additional cost, counted per commander.
- **Drannith Magistrate:** "can't cast spells from anywhere other than their hands".
- **Activate only as a sorcery**, "only once each turn", loyalty (one per turn per planeswalker, CR 606.3).

## Test scenarios
See the [[Tricky Interactions Catalog]] entries tagged `casting`.

## Related
[[Costs & Mana]] · [[Targeting & Legality]] · [[Decision & Action Model]]
