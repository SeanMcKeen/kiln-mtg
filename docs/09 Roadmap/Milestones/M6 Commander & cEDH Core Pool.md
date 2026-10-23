---
type: milestone
status: in-progress
milestone: M6
tags: [milestone, commander, cedh]
updated: 2026-10-23
---

# M6: Commander & cEDH Core Pool

**Goal:** complete Commander rules for 4-player pods, with the curated cEDH pool fully verified.

## Checklist
- [ ] Commander variant: command zone, tax, zone replacement and SBA, commander damage, color identity, partners ([[Commander]])
- [ ] Multiplayer: leaving the game, attack multiple players, APNAP everywhere ([[Multiplayer]])
- [ ] All [[Curated cEDH Core Pool]] cards `verified`
- [ ] Four representative cEDH-style test decks built from the pool (basics as filler where needed)
- [ ] Tricky catalog: T01–T14, T20–T28, T40–T47 passing
- [ ] Hidden-information auditor in CI

## Exit criteria
Thousands of full 4-player games with random and greedy policies: clean, deterministic, every win type observed at least once.

## Starting point (2026-10-23)
- The command zone exists as a zone id only: commanders, tax, commander damage, and "your commander's color identity" are gated in `support.rs` with "(M6)".
- Leaving the game (CR 800.4a) removes owned objects, but objects the player controls without owning aren't exiled, and control effects giving them control don't end.
- Curated pool: 202 cards. **89 verified, 14 parsed but unverified, 99 unsupported.** Almost all of the unsupported cards are "no parser template", so they need hand-written IR, and about 25 of them need new engine mechanics.
- Policies: random and greedy exist. The view auditor exists (`mtg dev fuzz --views`) but doesn't run in CI.

## Plan
Expected outcomes for new cards come from Oracle rulings in Scryfall's rulings bulk file (2026-10-22), paraphrased in scenarios and the catalog, never copied.

### Design decisions
| # | Decision | Why |
|---|---|---|
| C1 | **Commander designation is a property of the card, not a characteristic** (CR 903.3). `GameObject.commander: Option<u8>` (the slot in its owner's commander list) is carried when the card moves to a new zone. Tokens, copies and copy effects never get it. | Spark Double copying a commander isn't a commander (T45). The designation follows the card through zones (903.9). |
| C2 | **Setup:** `GameSetup` gains per-seat commanders (0–2). `GameRules::commander()` sets 40 life (CR 103.4c). Commanders start in the command zone (903.6). A player's color identity is the union of their commanders' identities (702.124c), stored at setup. | One setup path for scenarios, fuzzing and the sim. |
| C3 | **Casting from the command zone** (903.8): a cast option for each commander there, with the tax ({2} per previous cast from the command zone, tracked per commander) as an additional cost, so cost increases and alternative costs combine per 601.2f (T24). | Matches the CR's cost ordering. |
| C4 | **Returning to the command zone is always the owner's choice.** 903.9a is an SBA that offers a yes/no decision once per zone entry. 903.9b is a replacement that the owner may apply (it can apply more than once to the same event, an exception to 614.5). Both are `DecisionKind::CommanderZone` decisions; nothing is auto-resolved. | Legality by construction: these are player choices. |
| C5 | **Commander damage:** combat damage dealt by a commander is tallied per (victim, commander). 21 or more from one commander is a loss (704.6c, new `LossReason::CommanderDamage`). Partners keep separate tallies (T42). | 903.10a. |
| C6 | **"Your commander" references:** `Condition::ControlCommander` means a commander on the battlefield you control (903.3d). `ManaSpec::CommanderIdentity` produces colors in the identity; with no commander that part does nothing (903.4f). | Fierce Guardianship, Deadly Rollick, Deflecting Swat, Command Tower, Arcane Signet. |
| C7 | **Leaving the game, in full** (800.4a): owned objects leave; effects giving the player control of objects or players end; their non-card objects on the stack cease to exist; anything they still control is exiled. Choices they'd make go to the object's controller (800.4g). Costs they'd pay aren't paid (800.4f). | T44. |
| C8 | **Concession is out-of-band:** `Game::concede(player)` can be called at any time (CR 104.3a), like a real player standing up. It isn't offered as an option at every decision, which would double every option list. The sim harness and scenarios use it. | Exposed, but not bloating the decision model. Documented here as an explicit choice. |
| C9 | **Per-turn spell history:** each spell cast this turn records caster, colors, types and mana value. It drives storm (702.40), Kraum ("second spell each turn"), Rule of Law, Mindbreak Trap, Veil of Summer, Aetherflux Reservoir, Talion and Faerie Mastermind (draws are counted the same way). | One history, many readers. |
| C10 | **Mana with restrictions and riders** (106.6): mana units carry an optional restriction (creature spells only; legendary spells only, with "can't be countered"). Payment only offers legal units. Mana whose type depends on other objects (Chrome Mox, Mox Amber, Fellwar Stone, Exotic Orchard) and variable amounts (Gaea's Cradle, Food Chain, Rite of Flame) are computed at activation. Triggered mana abilities (605.1b) resolve immediately (Wild Growth, Utopia Sprawl). | Food Chain (T11), Delighted Halfling, the moxen. |
| C11 | **Abilities that work from other zones** (113.6): activated abilities can name the zone they work from (hand for channel and the Spirit Guides). Pregame actions (103.6) are a decision after mulligans, in turn order starting with the starting player (Leyline of the Void, Gemstone Caverns). | Otawara, Boseiju, Spirit Guides, Leyline, Caverns. |
| C12 | **Minimal planeswalkers:** loyalty abilities (606.3: sorcery timing, once per turn), enters with loyalty, damage removes loyalty (120.3c), 704.5i, and attacking planeswalkers (508.1b). | Jace, Wielder of Mysteries is in the pool. Once a planeswalker exists, attacking it is a legal choice and must be offered. |
| C13 | **Casting features:** kicker (702.33), overload (702.96), split second (702.61), dash (702.109), strive (an extra cost per extra target), Phyrexian mana, conditional alternative costs, cost increases (Thalia) and reductions (Power Artifact, channel), "you may cast that card this turn" permissions (Ragavan, Silas Renn, Eternal Scourge), and casting from the top of the library with life (Bolas's Citadel). | Each is needed by at least one pool card. |
| C14 | **Turn control** (723.1, Mindslaver) reuses the CR 723 decision routing from M5 for a whole turn. **Ending the turn** (724.1, Sundial) follows the CR steps. **Extra combat phases** (500.8, Najeela). **Voting** (701.38, Tivit) is a decision for each player in turn order. | Commanders and torture cards in the pool. |
| C15 | **Win types counted by the fuzzer:** life 0, commander damage, drawing from an empty library, an effect win (Thassa's Oracle, Laboratory Maniac, Jace), an effect loss, and a draw (all remaining players lose simultaneously, 104.4a, or a mandatory loop). Poison and concession can't occur with this pool and these policies; they're tested by scenarios instead. | Makes the exit criterion checkable. |

### Phases
| Phase | Content | Evidence |
|---|---|---|
| 6.1 | Commander core (C1–C6): setup, command zone casting with tax, 903.9a/b choices, commander damage, color identity mana, "control a commander". Cards: Command Tower, Arcane Signet, Fierce Guardianship, Deadly Rollick, Rograkh, Thrasios. | T41, T42 (with Kraum in 6.4), T43, T46, T47; scenarios |
| 6.2 | Multiplayer (C7, C8): leaving the game in full, concession, re-routed choices; the attack-multiple-players audit. "Can't lose / can't win" (Platinum Angel, T02), Gilded Drake (exchange of control). | T02, T44 |
| 6.3 | Mana (C10, C11): shocks, duals, City of Brass, Llanowar Wastes, monoliths, Mox Opal, Mox Amber, Chrome Mox, Fellwar Stone, Exotic Orchard, Gaea's Cradle, Rite of Flame, Spirit Guides, Lion's Eye Diamond, Wild Growth, Utopia Sprawl, Carpet of Flowers, Delighted Halfling, Food Chain, Gemstone Caverns, Otawara, Boseiju. | T11 (with Eternal Scourge) |
| 6.4 | Casting and the stack (C9, C13): spell history, storm, split second, kicker, overload, dash, strive, Phyrexian mana, conditional alternative costs, cost changes, cast permissions, Stifle, Deflecting Swat, Chain of Vapor, Pyroblast, Red Elemental Blast, Force of Negation, Snuff Out, Mindbreak Trap, Flusterstorm, Brain Freeze, Cyclonic Rift, Veil of Summer, Gitaxian Probe, Kraum, Silas Renn, Ragavan. | T05 (Breach + LED + Brain Freeze), T24 |
| 6.5 | Stax and graveyard hate: Null Rod, Collector Ouphe, Cursed Totem, Rule of Law, Drannith Magistrate, Silence, Orim's Chant, Thalia, Grafdigger's Cage, Rest in Peace, Leyline of the Void, Tormod's Crypt, Containment Priest, Aven Mindcensor, Archivist of Oghma, Talion. | T20–T23, T25, T26, T28, T40 |
| 6.6 | Library and draw engines (C12): Ad Nauseam, Angel's Grace, Timetwister, Peer into the Abyss, Sylvan Library, Sensei's Divining Top, Bolas's Citadel, Hermit Druid, Doomsday, Tainted Pact, Wishclaw Talisman, Faerie Mastermind, Malcolm, planeswalkers and Jace. | T13, T14 |
| 6.7 | Combo pieces and the rest (C14): Pestermite, Deceiver Exarch, Twinflame, Rings of Brighthearth, Power Artifact, Displacer Kitten, Worldgorger Dragon (Q26, Q27), Aetherflux Reservoir, Allosaurus Shepherd, Spark Double, Mirror Gallery, Mindslaver, Sundial of the Infinite, Najeela, Tivit. | T06, T07 (Twinflame), T10, T12, T45 |
| 6.8 | Four 100-card cEDH-style decks from the pool; Commander fuzzing with random and greedy policies; the view auditor in CI; win-type accounting; NFR re-baseline on real decks → review. | Exit criteria |

Power budget: builds with `-j 2`, tests with 2 threads, fuzz batches of at most 1,000 games on 2 threads per run. The exit-criterion run ("thousands of games") is split into batches of that size.

## Progress
| Phase | Status | Evidence |
|---|---|---|
| 6.1 | ✅ 2026-10-23 | **Commander core** (`commander.rs`):<br>- Setup: per-seat commanders start in the command zone (903.6); `GameRules::commander()` gives 40 life (103.4c).<br>- Casting from the command zone with the tax (903.8), added to whatever cost is paid.<br>- 903.9a (an SBA offer, once per entry into a graveyard or exile) and 903.9b (a replacement asked before every move into a hand or library, including draws) as `commander_zone` decisions; never auto-resolved.<br>- Commander damage per (victim, commander) and the 704.6c loss; partners keep separate tallies.<br>- `control_commander` (903.3d) and `commander_identity` mana (903.4f: none without a commander) via dynamic mana abilities.<br>- Views show the command zone and which objects are commanders. Invariant 9 checks the commander table and that no move skipped the 903.9b question.<br>- New IR: `reveal`, `battlefield_tapped`, `top_of_library_matches`.<br>- Fuzzer: `--commander` (a fitting legendary creature per seat), and per-game endings (loss reasons, effect wins, last standing).<br>Catalog: T41, T42 (synthetic partners; Kraum and Tymna come in 6.4), T43 pass; T46 and T47 were already covered (deck check, `setup_rules.rs`). 11 synthetic scenarios, 5 real. Verified: Command Tower, Arcane Signet, Fierce Guardianship, Deadly Rollick, Thrasios, Rograkh (171 total). Goldens re-blessed: only `pool_hash` changed (definitions gained color identity); choices and state hashes are identical. Fuzz: 500 synthetic plus 500 verified-card commander games with views and replays, clean; commander damage is the most common synthetic loss. |
| 6.2 | ✅ 2026-10-23 | **Leaving the game** in full (CR 800.4a): control effects giving the player control end (`end_effects`, shared with cleanup), delayed triggers go, and anything still controlled is exiled. **Concession** (104.3a): `Game::concede`, replays record it, scenarios have `concede: P2`, and the fuzzer has `--concede` (per mille). After a concession the pending decision is derived again, and procedures that can no longer continue end (gap: Q29).<br>**Player rules** "cant
lose and opponents
cant win" as statics (Platinum Angel); SBAs skip players who can't lose, and 704.5b now resets at each check. **Exchange of control** (701.12) plus `resolves_anyway` for Gilded Drake. Rule statics are classified in one place (`is_rule_static`).<br>Catalog T02 (Oracle vs. Platinum Angel) and T44 (concession after a Drake exchange, spell on the stack) pass. Verified: Platinum Angel, Gilded Drake (173 total).<br>Fuzz: 500 synthetic Commander games with 978 concessions plus 500 verified-card games with 932, invariants, views and 50 replays each: clean.<br>Build note: macOS refused to hard-link `target/` artifacts carrying another process's `com.apple.provenance` attribute; deleting the stale (pre-Oct-21) artifacts fixed it. |
