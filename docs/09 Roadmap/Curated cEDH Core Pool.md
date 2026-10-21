---
type: reference
status: draft
tags: [roadmap, cedh, cards, testing]
updated: 2026-10-20
---

# Curated cEDH Core Pool

This is the first card target ([[M6 Commander & cEDH Core Pool]]) and the benchmark set for the [[Card Definition Spike Plan]]. Cards are chosen for two things: (a) **frequency in cEDH**, and (b) **the rules systems they exercise**.

> [!check] Validated
> Every name below was checked against the Scryfall `oracle_cards` data dated 2026-10-20 and is **Commander-legal**, except the "negative tests" group. ★ marks Commander **Game Changers** (the Scryfall `game_changer` flag).
>
> Two cards were replaced. `Llanowar Elves` resolved to a non-legal duplicate-name object (see [[Ingestion Pipeline]]), and `Fable of the Mirror-Breaker` is a DFC whose Oracle name differs.

## Commanders (8)
| Card | Systems exercised |
|---|---|
| Kinnan, Bonder Prodigy | Static mana trigger ("tap a nonland permanent for mana → add one additional"), activated ability with a look-at-top choice |
| Thrasios, Triton Hero | Partner, activated ability, reveal-and-branch |
| Tymna the Weaver | Partner, postcombat-main trigger counting opponents dealt combat damage, pay life per draw |
| Kraum, Ludevic's Opus | Partner, "whenever an opponent casts their second spell each turn" (turn history) |
| Najeela, the Blade-Blossom | Attack trigger creating tokens tapped and attacking, extra combat with untap |
| Tivit, Seller of Secrets | Council's dilemma voting (each player votes, starting with the controller and proceeding in turn order), ward |
| Rograkh, Son of Rohgahh | Partner, no mana cost (MV 0), color indicator that sets color identity (CR 903.4) |
| Silas Renn, Seeker Adept | Partner, combat damage trigger, cast from graveyard |

## Fast mana & mana (≈ 30)
Sol Ring · Mana Vault★ (doesn't untap, upkeep choice, draw-step damage) · Grim Monolith★ · Basalt Monolith · Chrome Mox★ (imprint, optional) · Mox Diamond★ (ETB replacement with choice) · Mox Amber (conditional legend mana) · Mox Opal (metalcraft) · Lotus Petal · Lion's Eye Diamond★ (discard-hand cost, instant-only mana ability) · Arcane Signet · Fellwar Stone (opponent-dependent colors) · Talisman of Dominance (pain) · Dark Ritual · Cabal Ritual (threshold) · Rite of Flame (graveyard count) · Simian Spirit Guide · Elvish Spirit Guide (exile from hand as a cost) · Birds of Paradise · Elvish Mystic · Fyndhorn Elves · Delighted Halfling · Carpet of Flowers (opponent-dependent) · Wild Growth · Utopia Sprawl (aura mana triggers) · Ancient Tomb★ · Gaea's Cradle★ · Gemstone Caverns (pregame action, luck counter) · City of Brass · Mana Confluence · Command Tower · Exotic Orchard

## Lands (≈ 30)
Fetches: Polluted Delta, Flooded Strand, Misty Rainforest, Scalding Tarn, Verdant Catacombs, Bloodstained Mire, Wooded Foothills, Windswept Heath, Marsh Flats, Arid Mesa (search, pay life, shuffle, **knowledge invalidation**) · Duals: Underground Sea, Volcanic Island, Tropical Island, Bayou, Badlands, Taiga, Savannah, Scrubland, Tundra, Plateau (basic land types, Blood Moon interaction) · Shocks: Watery Grave, Steam Vents, Breeding Pool, Overgrown Tomb, Hallowed Fountain, Godless Shrine, Blood Crypt, Sacred Foundry, Stomping Ground, Temple Garden (ETB replacement with a choice) · Channel: Otawara, Soaring City, Boseiju, Who Endures (cost reduction, ability from hand) · Llanowar Wastes · Urborg, Tomb of Yawgmoth (layer 4) · Dryad Arbor (land creature) · basics.

## Tutors (≈ 14)
Demonic Tutor★ · Vampiric Tutor★ (top-of-library knowledge) · Imperial Seal★ · Mystical Tutor★ · Enlightened Tutor★ · Worldly Tutor★ · Gamble★ (random discard) · Diabolic Intent (sacrifice cost) · Grim Tutor · Merchant Scroll · Wishclaw Talisman (control passes to an opponent) · Tainted Pact (repeat-until loop) · Demonic Consultation (name a card).

## Interaction (≈ 30)
Counters: Force of Will★, Force of Negation, Fierce Guardianship★, Pact of Negation, Flusterstorm (storm), Mindbreak Trap (conditional free cast), Swan Song, Dispel, Spell Pierce, Counterspell, Mana Drain (delayed mana), An Offer You Can't Refuse, Mental Misstep, Stifle (counter an ability), Red Elemental Blast, Pyroblast (any-target legality nuance) · Redirection: Deflecting Swat (change targets, free with commander) · Removal: Swords to Plowshares, Pongify, Rapid Hybridization, Chain of Vapor (copy-on-sacrifice chain), Snuff Out (alt cost), Deadly Rollick, Abrupt Decay (can't be countered), Assassin's Trophy, Nature's Claim, Cyclonic Rift★ (overload), Toxic Deluge (X life) · Protection: Veil of Summer, Teferi's Protection★ (phasing, life can't change), Silence, Orim's Chant (kicker), Grand Abolisher · Graveyard hate: Tormod's Crypt, Rest in Peace, Leyline of the Void (pregame), Grafdigger's Cage.

## Stax & taxing (≈ 15)
Rule of Law · Drannith Magistrate★ · Opposition Agent★ (controls an opponent's search, CR 723) · Collector Ouphe · Null Rod · Cursed Totem · Thalia, Guardian of Thraben · Aven Mindcensor (search replacement) · Notion Thief★ (draw replacement) · Archivist of Oghma (search trigger) · Torpor Orb (ETB triggers) · Blood Moon (layer 4) · Dress Down (layer 6) · Talion, the Kindly Lord (number choice) · Esper Sentinel (unless-pay trigger).

## Card advantage & engines (≈ 20)
Rhystic Study★ · Mystic Remora (cumulative upkeep) · Smothering Tithe★ (Treasure tokens) · Orcish Bowmasters★ (flash ETB plus draw triggers, Amass) · Necropotence★ (skip, delayed, replacement) · Ad Nauseam★ · Brainstorm · Ponder · Preordain · Gitaxian Probe (Phyrexian mana) · Windfall · Wheel of Fortune · Timetwister · Peer into the Abyss · Sylvan Library (choice per card, pay life) · Night's Whisper · Sensei's Divining Top · Bolas's Citadel★ · Faerie Mastermind · Malcolm, Alluring Scoundrel · Ragavan, Nimble Pilferer (dash, exile-and-cast).

## Combo pieces & wincons (≈ 30)
Thassa's Oracle★ · Laboratory Maniac · Jace, Wielder of Mysteries (replacement win) · Underworld Breach★ · Brain Freeze · Isochron Scepter · Dramatic Reversal · Dualcaster Mage · Twinflame · Heat Shimmer · Kiki-Jiki, Mirror Breaker · Zealous Conscripts · Pestermite · Deceiver Exarch · Walking Ballista · Heliod, Sun-Crowned · Food Chain (restricted mana) · Eternal Scourge · Displacer Kitten · Power Artifact · Rings of Brighthearth (copy an activated ability) · Worldgorger Dragon · Animate Dead (aura reanimation, mandatory loop) · Doomsday (pile construction) · Aetherflux Reservoir (life-based activated ability) · Angel's Grace · Hermit Druid · Allosaurus Shepherd · Gilded Drake (exchange control).

## Rules-torture extras (not cEDH staples, kept for test coverage)
Humility★ · Opalescence · Clone · Phantasmal Image · Spark Double · Mirror Gallery · Mindslaver (CR 723.1) · Platinum Angel · Containment Priest · Sundial of the Infinite (CR 724) · Lightning Bolt · Grizzly Bears.

## Negative tests: banned in Commander (deck validation must reject)
Mana Crypt · Dockside Extortionist · Jeweled Lotus · Hullbreacher. Their status is per the 2026-10-20 Scryfall `legalities`, never hard-coded.

## Test decks (M6)
Build four cEDH-*style* 100-card decks from this pool, one per commander pair, with basics as filler. They exist to exercise rules, not to be competitive lists. **Real competitive lists** arrive in M11 as coverage grows ([[cEDH Ground-Truth Data]]).

## Related
[[Tricky Interactions Catalog]] · [[Card Definition Spike Plan]] · [[Commander]]
