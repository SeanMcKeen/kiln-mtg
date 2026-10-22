---
type: reference
status: draft
tags: [rules-map, testing, cedh, interactions]
updated: 2026-10-22
---

# Tricky Interactions Catalog

These seed scenarios become [[CR-Cited Scenario Tests]]. They are weighted toward **cEDH** because that's the headline use case. Each entry turns into a scenario file ([[Scenario File Format]]) during the milestone that owns its subsystem.

> [!warning] Confidence column
> **✔** means a well-established interaction, documented in Oracle rulings or the CR. **⚠** means the expected outcome is written from understanding and **must be verified** against Oracle rulings (Scryfall `rulings` bulk file) or a judge resource before the test is blessed. A wrong expected outcome in a test is worse than no test.

## Combo kills & alternate wins
| ID | Cards | Situation → expected | CR | Subsystems | Conf. |
|---|---|---|---|---|---|
| T01 | Thassa's Oracle + Demonic Consultation | Consultation naming an absent card exiles the whole library. The Oracle ETB resolves with X ≥ 0 = library size, so **the player wins**. Removing the Oracle in response does *not* stop it (devotion 0 ≥ 0 cards). | 104.2b, 608.2h | [[Resolution]], [[Game End, Restarts & Subgames]] | ✔ |
| T02 | T01 vs. an opponent's Platinum Angel | "Opponents can't win" means the trigger's win does nothing and the game continues ("can't" wins). | 101.2 | [[Game End, Restarts & Subgames]] | ✔ |
| T03 | Thassa's Oracle under **Dress Down** or **Torpor Orb** | The ETB doesn't trigger. Dress Down removes the ability in layer 6 as the Oracle enters. Torpor Orb stops ETB triggers. | 603.6a, 603.10, 613.1f | [[Triggered Abilities]], [[Continuous Effects & Layers]] | ⚠ |
| T04 | Thassa's Oracle + Tainted Pact (singleton deck) | Pact can exile the entire library (no duplicate names), then Oracle wins. | 608.2c | [[Resolution]] | ✔ |
| T05 | Underworld Breach + Lion's Eye Diamond + Brain Freeze | Escape-cast LED, crack it for BBB in response to nothing, escape-cast Brain Freeze with growing storm copies. **Requires shortcut support**, the storm count, and an LED mana ability activated while casting. | 605.3a, 702.40a, 702.138a, 732.2a | [[Casting & Activating]], [[Copy Effects]], [[Loops & Shortcuts]] | ✔ |
| T06 | Isochron Scepter (imprinted Dramatic Reversal) + ≥3 mana from nonland permanents | An optional loop for arbitrarily large mana and untaps. Shortcut to N iterations. | 707.12, 732.2a | [[Loops & Shortcuts]], [[Costs & Mana]] | ✔ |
| T07 | Dualcaster Mage + Twinflame / Heat Shimmer | An optional loop of hasty token copies. Copies of spells aren't cast. | 707.10, 707.10c | [[Copy Effects]], [[Loops & Shortcuts]] | ✔ |
| T08 | Kiki-Jiki + Zealous Conscripts / Pestermite / Deceiver Exarch | Arbitrarily many hasty tokens. Control change plus untap. | 707, 613.1b | [[Copy Effects]], [[Control & Ownership]] | ✔ |
| T09 | Walking Ballista + Heliod, Sun-Crowned | Lifelink ping, gain life, +1/+1 counter. A net-zero loop that drains everyone. | 613.1f, 613.4c, 603 | [[Counters]], [[Loops & Shortcuts]] | ✔ |
| T10 | Basalt Monolith + Rings of Brighthearth / Power Artifact | Arbitrarily large colorless mana. Rings copies the *untap* ability, which isn't a mana ability. | 605.1a, 707.10 | [[Costs & Mana]] | ✔ |
| T11 | Food Chain + Eternal Scourge | Arbitrarily large **restricted** mana (creature spells only). Tests conditional mana. | 106.6 | [[Costs & Mana]] | ⚠ |
| T12 | Worldgorger Dragon + Animate Dead, no other creature cards in any graveyard and no other actions | A **mandatory loop**, so the game is a draw. If any player can take an optional action that breaks the loop (another legal Animate Dead target, an instant), it isn't a draw. | 104.4b, 732.4–732.5 | [[Loops & Shortcuts]] (mandatory-loop draws exist since M4 4.7; detecting this zone-changing loop waits for Q26, M5) | ⚠ |
| T13 | Ad Nauseam + Angel's Grace | Reveal-and-lose-life repeats as a *choice* each iteration. At ≤ 0 life, Angel's Grace prevents the loss. | 104.3b, 101.2 | [[Resolution]] | ✔ |
| T14 | Bolas's Citadel + Sensei's Divining Top | Pay life to cast Top from the top of the library, tap it to draw and put it back. Repeat. | 601.2f | [[Casting & Activating]] | ✔ |

## Stax, taxes & "can't"
| ID | Cards | Situation → expected | CR | Subsystems | Conf. |
|---|---|---|---|---|---|
| T20 | Grand Abolisher | During its controller's turn, opponents get **no** cast options, and no activation options for artifacts, creatures or enchantments. Lands' mana abilities stay available. | 101.2 | [[Priority & the Stack]] | ✔ |
| T21 | Silence / Orim's Chant (kicked) | "Players can't cast spells this turn", typically cast on upkeep to protect a combo. | 101.2 | [[Priority & the Stack]] | ✔ |
| T22 | Drannith Magistrate | Opponents can't cast commanders from the command zone, Breach escapes, or flashback. | 101.2 | [[Casting & Activating]] | ✔ |
| T23 | Rule of Law | One spell per turn per player, tracked in turn history. | 101.2 | [[Priority & the Stack]] | ✔ |
| T24 | Thalia, Guardian of Thraben + commander tax + Force of Will alt cost | Cost increases apply to alternative costs too. Total-cost ordering. | 601.2f, 903.8 | [[Costs & Mana]] | ✔ |
| T25 | Grafdigger's Cage vs. Underworld Breach / Animate Dead | Cage stops casting from graveyards and creature cards entering from graveyards or libraries. | 101.2 | [[Casting & Activating]] | ✔ |
| T26 | Aven Mindcensor + Demonic Tutor | The search is restricted to the top four cards (replacement). | 614 | [[Replacement & Prevention]] | ✔ |
| T27 | Opposition Agent + an opponent's Demonic Tutor | Agent's controller **controls the opponent during the search**, chooses (or fails to find), and the found card is exiled and playable by the Agent's controller. | 723.2, 723.5 | [[Control & Ownership]] | ✔ |
| T28 | Null Rod / Collector Ouphe + Sol Ring | Artifact activated abilities, including mana abilities, can't be activated. | 101.2 | [[Costs & Mana]] | ✔ |

## Triggers, replacements & draws
| ID | Cards | Situation → expected | CR | Subsystems | Conf. |
|---|---|---|---|---|---|
| T30 | Rhystic Study / Mystic Remora / Esper Sentinel / Smothering Tithe (multiple controllers) | One opponent action triggers several players' abilities. APNAP order, each player orders their own. The "unless pays" choice goes to the acting player. | 603.3b, 101.4 | [[Triggered Abilities]] | ✔ |
| T31 | Notion Thief (P1) + Windfall cast by P2 | P2's draws beyond the first in their draw step are replaced, so P1 draws instead. | 614 | [[Replacement & Prevention]] | ✔ |
| T32 | Orcish Bowmasters (P3) + T31 | Every replaced draw *by P1* (an opponent of P3) triggers Bowmasters. A massive trigger batch, so test performance. | 603.2 | [[Triggered Abilities]] (checked against the 2023-06-16 Bowmasters and 2018-03-16 Notion Thief rulings; scenario `t32-bowmasters-notion-windfall`) | ✔ |
| T33 | Necropotence | Skip the draw step. Pay 1 life to exile the top card face down, then a delayed trigger puts it into hand at the next end step. Discards are exiled instead. | 603.7, 614 | [[Replacement & Prevention]], [[Triggered Abilities]] | ✔ |
| T34 | Mox Diamond | "If ~ would enter, you may discard a land card instead. If you do, put ~ onto the battlefield. If you don't, put it into its owner's graveyard." An ETB replacement with a choice. | 614.1c | [[Replacement & Prevention]] | ✔ |
| T35 | Pact of Negation | The delayed upkeep trigger: pay {3}{U}{U} or lose. If unable, lose. | 603.7 | [[Triggered Abilities]] | ✔ |
| T36 | Mana Drain | Delayed trigger: add {C} equal to the countered spell's MV at the start of the next main phase. | 603.7 | [[Triggered Abilities]] | ✔ |
| T37 | Mana Vault | Doesn't untap during untap. Upkeep: may pay {4} to untap. Draw step: 1 damage if tapped. | 502.3 | [[Turn Structure]] | ✔ |

## Commander & multiplayer
| ID | Cards | Situation → expected | CR | Subsystems | Conf. |
|---|---|---|---|---|---|
| T40 | Commander dies with Rest in Peace on the battlefield | RIP's replacement exiles it. Then the SBA (903.9a) lets the owner move it to the command zone. | 903.9a, 704.3 | [[Commander]] | ✔ |
| T41 | Commander bounced to hand | The 903.9b replacement means the owner *may* send it to the command zone instead. It can apply repeatedly. | 903.9b | [[Commander]] | ✔ |
| T42 | Tymna + Kraum (partners), combat damage | Separate commander-damage tallies, so 21 from **one** commander is needed. | 903.10a, 704.6c | [[Commander]], [[Combat]] | ✔ |
| T43 | Fierce Guardianship | Free if you control a commander. Checks "your commander" (903.3d). | 903.3d | [[Casting & Activating]] | ✔ |
| T44 | Player concedes with a spell on the stack and a stolen Gilded Drake | Their stack objects cease to exist, control effects end, and remaining controlled objects are exiled. | 800.4a | [[Multiplayer]] | ✔ |
| T45 | Spark Double copying a commander | Not legendary (by its exception). Not a commander, because designation isn't copiable. | 707.9, 903.3 | [[Copy Effects]], [[Commander]] | ✔ |
| T46 | Deck with Dockside Extortionist / Mana Crypt | Deck validation **rejects** it (banned in Commander per current data). | 903.5 | [[Commander]] | ✔ |
| T47 | Multiplayer mulligans | The first mulligan is free (103.5c), then London bottoming. | 103.5, 103.5c | [[Turn Structure]] | ✔ |

## Layers & copies (rules torture)
| ID | Cards | Situation → expected | CR | Subsystems | Conf. |
|---|---|---|---|---|---|
| T50 | Blood Moon + Urborg, Tomb of Yawgmoth | Dependency: Urborg's effect depends on Blood Moon, so Urborg is just a Mountain and other lands aren't Swamps. | 613.8 | [[Continuous Effects & Layers]] | ✔ |
| T51 | Humility + Opalescence (both timestamp orders) | The classic. The outcome differs by timestamp. The expected P/T for each order must be taken from the Oracle rulings. | 613.6, 613.7 | [[Continuous Effects & Layers]] | ⚠ |
| T52 | Blood Moon + Dryad Arbor | Arbor keeps creature and Forest types? Verify. Layer 4 subtype replacement. | 613.1d | [[Continuous Effects & Layers]] | ⚠ |
| T53 | Phantasmal Image copying a creature, then targeted | It's sacrificed via its copied-plus-added trigger (707.9a). | 707.9a | [[Copy Effects]] | ✔ |
| T54 | Toxic Deluge X = 3 | -3/-3 locked in on the creatures present on resolution (611.2c). New creatures are unaffected. | 611.2c, 613.4c | [[Continuous Effects & Layers]] | ✔ |
| T55 | Teferi's Protection | Life total can't change, protection from everything, phases out all permanents. Attacks against the player are impossible, and the trigger interactions are checked. | 702.26, 702.16 | [[Keywords & Keyword Actions]] | ✔ |

## Randomness & hidden information
| ID | Cards | Situation → expected | CR | Subsystems | Conf. |
|---|---|---|---|---|---|
| T60 | Gamble | Search, then **random** discard. Seeded, so replay-identical. | 701.24, 705 | [[Randomness]] | ✔ |
| T61 | Vampiric Tutor | Opponents learn "one card on top, unknown". The tutoring player knows it. Knowledge tracking. | 701.24a | [[Hidden Information & Player Views]] | ✔ |
| T62 | Brainstorm then shuffle | The two put-back cards are known to the owner until the shuffle. | 701.24a | [[Hidden Information & Player Views]] | ✔ |

## Related
[[Curated cEDH Core Pool]] · [[CR-Cited Scenario Tests]] · [[Loops & Shortcuts]]
