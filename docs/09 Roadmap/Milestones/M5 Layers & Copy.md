---
type: milestone
status: in-progress
milestone: M5
tags: [milestone]
updated: 2026-10-22
---

# M5: Layers & Copy

## Checklist
- [ ] Layer system with sublayers, CDAs, timestamps, dependencies ([[Continuous Effects & Layers]])
- [ ] Characteristic caching and invalidation, plus a benchmark
- [ ] Copy effects: permanents, spells, abilities, cast-a-copy ([[Copy Effects]])
- [ ] Control change and controlling another player ([[Control & Ownership]])
- [ ] Tricky catalog: T03, T07, T08, T09, T27, T50–T55 passing (⚠ entries verified first)
- [ ] T12 Worldgorger Dragon mandatory loop (moved from M4): needs zone-changing loop detection, [[12 Open Questions]] Q26
- [ ] M4 leftovers: Auras and Equipment attachment (CR 303.4, 301.5; SBAs 704.5m–p), the layer-4 type change in amass, "becomes the target" for player targets

## Exit criteria
The layer torture suite passes. Characteristic computation fits the performance budget.

## Plan (2026-10-22)
The expected outcomes for the ⚠ catalog entries come from Oracle rulings fetched from Scryfall on 2026-10-22 (paraphrased; see the catalog):
- **T51:** an older Humility leaves Opalescence's creatures 4/4; an older Opalescence leaves them 1/1. Opalescence keeps applying in later layers after losing its ability (CR 613.6).
- **T52:** Dryad Arbor under Blood Moon is a green Mountain Dryad creature.
- **T03:** an Oracle that enters under Dress Down has no abilities, so its ETB doesn't trigger.

### Design decisions
| # | Decision | Why |
|---|---|---|
| L1 | **Characteristics are computed** (`Game::chars(id)`): name, mana value, card types, subtypes, supertypes, colors, abilities, keywords, P/T and controller. The printed values (the copiable values, CR 707.2) are transformed by the layers. Every rules read goes through it, or through narrow accessors (`types_of`, `colors_of`, `keywords`, `power`, `abilities_of`). | One source of truth for CR 613. Today about 80 call sites read card definitions directly. |
| L2 | **Abilities are referenced by `AbilityRef(def, index)`.** An object's ability list is computed, so it can include copied abilities (another definition's) and granted ones. `Action::Activate`/`ActivateMana` carry the `AbilityRef`. Stack items and triggers already store `(def, index)`. | Copy, grant and remove (Humility, Dress Down) need abilities that don't come from the object's own card. |
| L3 | **One continuous-effect store plus static abilities.** Effects from resolved spells and abilities are stored, each with a timestamp, duration, layer payload and affected set. Static abilities of permanents generate their effects at computation time, with the permanent's timestamp. The store replaces `pt_effects`. | CR 611.2 vs. 611.3: one-shot-created effects lock in their affected set (611.2c, Toxic Deluge). Static abilities re-evaluate continuously (611.3a). |
| L4 | **Timestamps** (CR 613.7): every object gets one when it enters a zone, and every effect when it's created, from one game counter. Goldens are re-blessed once, because the state hash gains timestamps. | Humility/Opalescence and Dress Down are decided by timestamp order. |
| L5 | **Layer order:** 1a copy, 1b face-down, 2 control, 3 text (rejected by `support.rs` for now), 4 type, 5 color, 6 abilities, 7a CDAs, 7b set, 7c modify (counters included), 7d switch. CDAs go first within a layer (613.3). Then dependencies (613.8), found by trial: A depends on B if applying B first changes A's existence, what it applies to, or what it does. Otherwise timestamp order; a dependency loop also falls back to timestamps. An effect that started in an earlier layer keeps applying in later ones (613.6). | The exact CR procedure. Trial application is cheap with the few effects a real board has, and handles Blood Moon + Urborg (T50) without special cases. |
| L6 | **Performance:** a fast path when no effect exists (characteristics equal printed values, as now). Otherwise a computed cache, refreshed between tasks when something marked it dirty; reads in between compute fresh. Budget: all characteristics of a 60-permanent board with 10 static effects in ≤ 20 µs (fits NFR-P3). Benchmarked. | No `RefCell` in `Game` (it must stay `Sync`-friendly for search). Correct at every read. |
| L7 | **Copy effects** (layer 1, CR 707): an object's copiable values become another object's copiable values, plus the exceptions in 707.9. Exception abilities come from the copier's definition, flagged as not its own abilities. Covers token copies (Kiki-Jiki, Twinflame) and copies of spells (CR 707.10: put on the stack, not cast; new targets optional per 707.10c). | T07, T08, T53. |
| L8 | **Control:** layer-2 effects change controllers, and `GameObject.controller` holds the computed result. Controlling a player (CR 723) is decision routing: `PendingDecision.player` is the controlling player, and the controlled player stays the actor. | T08 (Zealous Conscripts), T27 (Opposition Agent). |
| L9 | **Attachments:** Auras (enchant, CR 303.4) and Equipment (equip, CR 301.5) with `attached_to`, plus SBAs 704.5m–p. | M4 leftovers; Animate Dead for T12. |
| L10 | **Zone-changing loop detection (Q26):** in the loop hash, an object reference hashes as its arena index plus whether it's alive, so a card re-entering the same slot looks the same. | T12 (Worldgorger) as a mandatory-loop draw. |

### Phases
| Phase | Content | Evidence |
|---|---|---|
| 5.1 | Layer core: L1–L4 and L6. A refactor with no behaviour change: computed characteristics, `AbilityRef` lists, the effect store, timestamps, the fast path and cache, a benchmark. | All tests pass; goldens re-blessed for timestamps only; benchmark recorded |
| 5.2 | Statics in layers 4–7 (L5): set/add types, lose abilities, set base P/T, anthems; static conditions; dependencies and 613.6. Rule statics (`CantCast`/`CantActivate`, Torpor Orb). Real cards: Blood Moon, Urborg, Humility, Opalescence, Dress Down, Torpor Orb, Toxic Deluge, Grand Abolisher. | T03, T50, T51, T52, T54 |
| 5.3 | Copy effects (L7): Clone/Phantasmal Image, token copies, copies of spells. | T07, T53 |
| 5.4 | Control (L8): Zealous Conscripts, Kiki-Jiki; Opposition Agent (CR 723). | T08, T27 |
| 5.5 | Attachments (L9): Auras, Equipment, Animate Dead; the amass type change. | Scenarios |
| 5.6 | Teferi's Protection (phasing CR 702.26, protection for players), Walking Ballista + Heliod; zone-changing loops (L10). | T55, T09, T12 |
| 5.7 | The layer torture suite, cache benchmark, real cards, low-power fuzz → review. | Exit criteria |

## Progress
| Phase | Status | Evidence |
|---|---|---|
| 5.1 | ✅ 2026-10-22 | `layers.rs`, as a refactor:<br>- Computed characteristics (`Chars`: printed fast path, or computed through the layers) and `abilities_of` with `AbilityRef`s.<br>- The continuous-effect store with locked-in sets (CR 611.2c) replaces `pt_effects`; until-end-of-turn effects end in cleanup.<br>- Object and effect timestamps (CR 613.7).<br>- Rerouted through characteristics: `matches_filter`, "any target", protection, trigger matching, replacements and statics.<br>Goldens re-blessed: only `final_hash` changed (timestamps); every choice reproduces.<br>Throughput: an A/B on the same seed against the pre-M5 build found chars and trigger lookup 14% slower. Fixed by a fast path and by replacing the per-SBA scan of all objects for tokens (CR 704.5d) with a list of tokens that left the battlefield. Now 318 vs 314 games/s. Activated and mana abilities still come from the definition; they move to `AbilityRef` in 5.2, when abilities can differ. |
| 5.2 | ✅ 2026-10-23 | The layers are computed for the whole battlefield and cached per epoch:<br>- Layers 4 (set or add types, land types with intrinsic abilities per CR 305.6–305.7), 6 (lose and grant abilities), 7b (set base P/T) and 7c (pumps, anthems, counters).<br>- Dependencies by trial (613.8); multi-layer statics keep applying (613.6); locked sets for one-shot effects (611.2c).<br>- Rule statics: can't cast or activate, with conditions (Grand Abolisher); Torpor Orb.<br>- Toxic Deluge: X announced for an additional cost (107.3a), and signed quantity pumps.<br>Invariant 8 compares the cache with a fresh computation (it caught a missing epoch bump when a player leaves). Performance: rule statics are cached per epoch, with an empty-epoch fast path.<br>Catalog: T03 (Dress Down, Torpor Orb), T50, T51 (both orders), T52, T54 pass. 9 synthetic layer and rule scenarios. 152 verified. Fuzz: 1,000 library games with views plus 1,000 verified-card games with 20 replays, all clean. |
| 5.3 | ✅ 2026-10-23 | Copy effects (CR 707):<br>- **Layer 1a:** copy effects as stored effects carrying copiable values: the definition plus the 707.9 exceptions (types, subtypes, granted abilities, not legendary). The exceptions are copiable too, so a copy of a copy keeps them.<br>- **Granted abilities** are stored as hidden abilities on the granting definition; printed reads exclude them.<br>- **Enter as a copy** (Clone, Phantasmal Image): an optional pick; the copy effect exists before the zone-change event, so copied ETBs trigger.<br>- **Token copies** use a hidden copy-token definition, so token status stays correct (Heat Shimmer, Kiki-Jiki with a next-end-step sacrifice).<br>- **Copies of spells** are put on the stack, not cast, with a keep-or-change choice per target (Dualcaster Mage).<br>- **Names, legend rule, mana value and view labels** use the current (copied) definition; zone-change last-known information records it.<br>Catalog T07 and T53 pass. Verified: Phantasmal Image, Clone, Heat Shimmer, Dualcaster Mage, Kiki-Jiki (157 total). 2 synthetic scenarios. Fuzz: 500 library games with views plus 500 verified-card games with replays, all clean. |
| 5.4 | ✅ 2026-10-23 | **Control:**<br>- Layer-2 control effects keep the computed controller on the object. A controller change restarts summoning sickness (CR 302.6), and until-end-of-turn control reverts in cleanup.<br>- `GainControl` and `GainKeyword` (an until-end-of-turn grant in layer 6).<br>- A player who casts or plays another player's card controls it.<br>**Controlling a player** (CR 723): an Opposition Agent's controller makes the choices in an opponent's search (latest Agent wins, per the ruling). Found cards are exiled and linked; the Agent's controller may cast or play them, spending mana as though it were any color (colored symbols become generic).<br>Catalog T08 (Kiki-Jiki + Zealous Conscripts) and T27 pass. Verified: Zealous Conscripts, Opposition Agent (159 total). 2 synthetic scenarios. Fuzz: 500 library games with views plus 500 verified-card games with replays, all clean. |
| 5.5 | ✅ 2026-10-23 | **Attachments** (`attach.rs`):<br>- `Enchant` and `Equip` keywords; equip is a synthesized sorcery-speed activated ability.<br>- Aura spells target what they'll enchant, including cards in graveyards, and enter attached (CR 303.4a, 303.4f).<br>- The `attached` filter means "enchanted/equipped creature"; `ObjectRef::AttachedTo` still works after the source leaves (last known information).<br>- SBAs: 704.5m (Aura illegal or unattached → graveyard), 704.5n (illegal Equipment → unattached), 704.5p; protection makes an attachment illegal (702.16c).<br>- Animate Dead: `ReturnAttached`, with bound attachments for its changed enchant.<br>- Amass's type change, as a permanent layer-4 effect.<br>Verified: Animate Dead, Lightning Greaves, Swiftfoot Boots (162 total). 1 synthetic scenario. Fuzz: 500 + 500 games, clean. |
