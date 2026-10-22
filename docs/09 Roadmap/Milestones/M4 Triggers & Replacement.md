---
type: milestone
status: in-progress
milestone: M4
tags: [milestone]
updated: 2026-10-21
---

# M4: Triggers & Replacement

## Checklist
- [ ] Triggered abilities: APNAP, player-ordered, intervening-if, delayed, state triggers, look-back ([[Triggered Abilities]])
- [ ] Replacement and prevention with the full CR 616.1 procedure ([[Replacement & Prevention]])
- [ ] Remaining SBAs ([[State-Based Actions]])
- [ ] Loop detection and the CR 732 shortcut protocol, large-number strategy ([[Loops & Shortcuts]]) → resolve FR-E11
- [ ] Tricky catalog: T12, T30–T37 passing

## Exit criteria
Catalog entries pass. The Isochron/Reversal loop runs as a shortcut in under 1 ms.

## Plan (2026-10-21)
### Design decisions
| # | Decision | Why |
|---|---|---|
| D1 | **Triggers are detected from the event log.** Each logged event is matched against the trigger conditions of permanents on the battlefield (and of delayed triggers), indexed by event kind. Matches go into a pending queue in the state, carrying a snapshot of the trigger's source and the triggering object/player (CR 603.10 look-back). | One source of truth for "what happened" ([[Event System]]). Look-back falls out of the snapshot. |
| D2 | **Putting triggers on the stack is a task** that runs whenever a player would receive priority, after SBAs (CR 117.5, 603.3). APNAP order (CR 101.4); each player orders their own triggers by picking the next one to put on the stack (`OrderTriggers`, one-of-N). Targets and modes are chosen as each goes on the stack (CR 603.3c–d); a trigger with no legal targets is removed. | The user's named example of a customizable legal choice. Same factored contract as casting. |
| D3 | **Triggered abilities on the stack reuse the cast procedure** (modes, targets) and the interpreter, with `ObjectRef::Triggering` / `PlayerRef::Triggering` bound from the event. Intervening "if" is checked on triggering and on resolution (CR 603.4). Triggered mana abilities (CR 605.1b) resolve immediately. | One procedure for spells, activated and triggered abilities. |
| D4 | **Delayed triggers** (CR 603.7) are created by `Effect::Delayed` with the creating context captured (controller, remembered objects, X). One-shot unless the IR gives a duration. | Pact of Negation, Mana Drain, Necropotence. |
| D5 | **Replacement and prevention run on proposed events** for the event kinds cEDH needs: draw, zone change (including entering the battlefield, "enters tapped", "as ~ enters" choices), damage, and step skipping. CR 616.1's ordering is a `ChooseReplacement` decision when more than one applies; each effect applies at most once per event (CR 614.5). | The full CR 616 procedure without building a generic event algebra before it's needed. |
| D6 | **Rule statics needed by M4 cards** run as direct engine checks: "doesn't untap during your untap step" (CR 502.3) and "skip your draw step" (CR 614.1b). Other statics stay in M5 (layers). | Mana Vault and Necropotence. |
| D7 | **Ward** is a triggered ability (CR 702.21a); **protection** (CR 702.16) covers targeting, blocking and damage prevention. Attachment parts of protection arrive with Auras in M5. | Moved here from M3. |
| D8 | **Copies of cards and spells** (CR 707.10, 707.12) for Isochron Scepter: a copy is an object with its own definition reference and a `copy` flag; copies outside the stack (or battlefield, for card copies) cease to exist (SBA 704.5e). | The exit criterion's loop. |
| D9 | **Loops** (CR 732): the engine hashes the state *modulo monotonic quantities* (mana pools, life, counters, token counts) after each priority action of one player; a repeat with a non-decreasing delta offers that player a `ProposeShortcut` decision (repeat N times). Opponents may accept or shorten (CR 732.2b) in turn order. A shortcut applies the per-iteration delta N times when every iteration's delta is identical; otherwise it iterates. A loop of only mandatory actions is a draw (CR 104.4b, 732.4). | Option A + C from [[Loops & Shortcuts]]. |
| D10 | **Large numbers (FR-E11):** life, mana and counters stay `i64`/`u32` with saturating arithmetic; "arbitrarily large" means a shortcut N chosen by the proposer, capped at 10⁹ per shortcut. | Exact for every realistic game; Q15 resolved without a bignum type. |

### Phases
| # | Work | CR | Done when |
|---|---|---|---|
| 4.1 | Trigger engine: matching, pending queue, APNAP + `OrderTriggers`, targets/modes on stacking, intervening-if, triggered mana abilities, `Triggering` references, controller-left (CR 800.4d) | 603.2–603.6, 605.1b, 101.4 | Synthetic-card scenarios |
| 4.2 | Delayed and state triggers; leaves-the-battlefield look-back | 603.7, 603.8, 603.10 | Scenarios |
| 4.3 | Replacement/prevention pipeline (draw, zone change, ETB, damage), `ChooseReplacement`, "enters tapped", skip-step and doesn't-untap statics | 614–616, 502.3 | Scenarios |
| 4.4 | Ward and protection | 702.21, 702.16 | Scenarios |
| 4.5 | Remaining SBAs that apply to implemented objects (704.5e copies, 704.5r); the rest (planeswalkers, Auras, Sagas, battles) are listed for their milestones | 704.5 | Scenarios |
| 4.6 | Copies of cards and spells; linked "imprint" abilities; cast-without-paying | 707.10, 707.12, 607 | Isochron Scepter scenario |
| 4.7 | Loop detection, `ProposeShortcut`, mandatory-loop draw | 732, 104.4b | Isochron/Reversal shortcut < 1 ms; synthetic mandatory loop is a draw |
| 4.8 | Real cards and the catalog: Rhystic Study, Mystic Remora, Esper Sentinel, Smothering Tithe, Notion Thief, Orcish Bowmasters (amass), Necropotence, Mox Diamond, Pact of Negation, Mana Drain, Mana Vault, Isochron Scepter, Dramatic Reversal, and other trigger staples; low-power fuzz | T30–T37 | Catalog scenarios pass; fuzz clean |

**Moved:** T12 (Worldgorger Dragon + Animate Dead) needs Auras and attachment, so it moves to M5. M4 proves the mandatory-loop draw with a synthetic loop.

## Progress
| Phase | Status | Evidence |
|---|---|---|
| 4.1 | ✅ 2026-10-21 | `triggers.rs`: events matched against trigger conditions (even with logging off; trigger-free pools skip it), pending queue, `StackTriggers` after SBAs, APNAP with player-chosen `OrderTriggers`, targets via the cast procedure (no legal target: removed, CR 603.3d), intervening-if on trigger and resolution, `Triggering` references, look-back with simultaneous leave batches (CR 603.10a), departed controllers (CR 800.4d). 9 synthetic scenarios; Rhystic Study, Smothering Tithe and Thassa's Oracle verified (126 total). Library: 2,632 supported cards. Fuzz 3,000 games over the library: clean. Goldens: only `pool_hash` changed. |
| 4.2 | ✅ 2026-10-21 | Delayed triggers (`Effect::Delayed`, CR 603.7): created with the creating effect's context (targets and last known information), one-shot, controlled by the creator's controller. Triggered mana abilities (CR 605.1b, 605.4a): a `TappedForMana` event when a `{T}` mana ability adds mana; matches resolve at once without using the stack. Cumulative upkeep (CR 702.24a) built as a triggered ability with a `ManaRepeated` cost. Amass (CR 701.47a). Per-turn tracking for "first … each turn" and "opponents dealt combat damage this turn". `LoseGame` (CR 104.3e). `ManaSpec::Colorless(q)`. Scenarios can preload counters (`counters:` on a permanent). 8 synthetic scenarios; Mana Drain, Pact of Negation, Esper Sentinel, Mystic Remora, Orcish Bowmasters, Kinnan and Tymna verified (133 total). Library: 2,698 supported cards. Fuzz: 3,000 library games clean, plus 1,000 with 20 replays verified. Goldens unchanged. |

### Known limitations after 4.2
- **No targets on delayed triggers.** A delayed trigger can't choose targets of its own. No supported card needs this yet.
- **The auto-pay planner ignores Kinnan.** It doesn't count mana from triggered mana abilities, so the extra mana floats in the pool. That is legal but can underplay Kinnan. Manual payment sees everything.
- **Amass skips the type change.** It doesn't make an existing Army gain the named creature type (CR 701.47a, "becomes a [subtype] in addition"). That needs layer 4 (M5).
- **No state triggers yet** (CR 603.8). No supported card has one; they'll be added when one does.
