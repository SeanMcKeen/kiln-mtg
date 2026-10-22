---
type: subsystem
status: draft
cr: ["604", "611", "612", "613"]
impl_status: implemented
milestone: M5
tags: [subsystem, layers, continuous-effects]
updated: 2026-10-23
---

# Continuous Effects & Layers

## Purpose
This subsystem computes every object's current characteristics from its base values plus all applicable continuous effects, following the layer system exactly.

## CR sections
604 (static abilities, CDAs in 604.3), 611 (continuous effects), 612 (text-changing), 613 (interaction of continuous effects).

## The layers (CR 613.1, verified against the 2026-06-19 text)
| Layer | Applies |
|---|---|
| 1a | Copiable effects: copy effects (CR 707) and merge (CR 613.2a) |
| 1b | Face-down characteristics (CR 613.2b, 708.2) |
| 2 | Control-changing |
| 3 | Text-changing |
| 4 | Type-changing (card type, subtype, supertype) |
| 5 | Color-changing |
| 6 | Ability adding and removing, keyword counters, "can't have" |
| 7a | P/T from CDAs |
| 7b | P/T setting, and "base power/toughness" effects |
| 7c | P/T modifications, including counters |
| 7d | P/T switching |

After objects: player-affecting effects (CR 613.10), then rules-modifying effects (CR 613.11).

## Responsibilities
- Within layers 2–6, CDAs apply first (CR 613.3), then **timestamp order** (CR 613.7), **overridden by dependencies** (CR 613.8). Dependency means:
  - same layer or sublayer
  - and applying B changes the existence, text, applicability or effect of A (CR 613.8a)

  Dependency loops fall back to timestamps.
- Effects that span layers keep applying in later layers once started (CR 613.6). Example: Humility and Opalescence.
- **Locked-in vs. continuously evaluated sets** (CR 611.2c): effects from resolved spells lock in the affected set, while static abilities re-evaluate continuously.
- Caching and invalidation of derived characteristics ([[Performance & Cheap Cloning]]).

## Decisions raised
Usually none. The layer system is deterministic. Choices are made when effects are created, e.g. "choose a color".

## Edge cases (canonical test cases)
- **Blood Moon + Urborg, Tomb of Yawgmoth:** a layer-4 dependency. Blood Moon removes Urborg's ability, so Urborg's effect depends on Blood Moon.
- **Humility + Opalescence:** the classic dependency and timestamp interaction.
- **Dress Down:** removes creature abilities, including the abilities of other static effects.
- **Mirror Gallery:** a rules-modifying effect on the legend rule.
- **Phantasmal Image / Spark Double copying with exceptions:** layer 1 with "except" clauses (see [[Copy Effects]]).
- **Walking Ballista + Heliod, Sun-Crowned:** counters (7c) plus granted lifelink (6) plus triggers, a cEDH combo.
- **Dryad Arbor under Blood Moon.**

## Related
[[Copy Effects]] · [[Control & Ownership]] · [[Tricky Interactions Catalog]]
