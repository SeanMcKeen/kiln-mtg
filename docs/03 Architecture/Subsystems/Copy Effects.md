---
type: subsystem
status: draft
cr: ["707"]
impl_status: implemented
milestone: M5
tags: [subsystem, copy]
updated: 2026-10-23
---

# Copy Effects

## Purpose
This subsystem covers copying permanents, spells, abilities and cards: copiable values, exceptions, copies of copies, and casting copies.

## CR sections
707 (copying objects), plus layer 1 in [[Continuous Effects & Layers]] (CR 613.2).

## Responsibilities
- **Copiable values** (CR 707.2): printed values as modified by other copy effects, face-down rules, and the copy exceptions in 707.9 ("except it's legendary", "except it has haste").
- **Copies of spells and abilities** (CR 707.10) are put on the stack, **not cast**, so "cast" triggers don't fire. Optionally choose new targets (CR 707.10c).
- **Casting a copy** (CR 707.12): Isochron Scepter, Dualcaster Mage-style interactions, "cast a copy" effects. These *are* cast.
- **Token copies** (Kiki-Jiki, Twinflame, Heat Shimmer, Spark Double, Phantasmal Image).
- Storm (CR 702.40a) and Brain Freeze, the Breach line: many copies, which stress performance.

## Decisions raised
- Choose new targets for each copy (CR 707.10c).
- Choose what to copy (Clone "you may have it enter as a copy of any creature", an ETB replacement, CR 616.1c ordering).

## Edge cases
- Copying a DFC, a face-down permanent, or a copy with exceptions (Spark Double copying Spark Double).
- **Dualcaster Mage + Twinflame / Heat Shimmer:** an optional infinite loop of token copies with haste. It needs [[Loops & Shortcuts]].
- Copying a commander does **not** copy commander-ness (CR 903.3 "not a characteristic").

## Related
[[Continuous Effects & Layers]] · [[Loops & Shortcuts]] · [[Commander]]
