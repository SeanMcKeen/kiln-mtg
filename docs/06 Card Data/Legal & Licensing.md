---
type: reference
status: accepted
tags: [legal, licensing]
updated: 2026-10-20
---

# Legal & Licensing

> [!caution] Not legal advice
> These are the project's working understanding and policies. Revisit them if the project is commercialized or published widely.

## Project code
Dual-licensed **MIT OR Apache-2.0** ([[ADR-0002 Licensing and Card Data Distribution]]). Contributions are accepted under the same terms (inbound = outbound).

## Wizards of the Coast content
- Card names, Oracle text, mana symbols, card images, and the Comprehensive Rules are WotC IP. Use falls under the **Fan Content Policy**: free, non-commercial, unofficial, with the required disclaimer (in the [README](../../README.md)).
- **We don't redistribute** card data, images, or CR text in the repo or in release artifacts. Users fetch them. The local copies in `docs/_external/` and `data/cache/` are gitignored.
- Card names *in code and tests* (e.g. a scenario that uses "Thassa's Oracle") are common practice in fan projects (Forge, XMage, phase.rs). Keep it to references, and never ship bulk Oracle text.
- Don't use WotC logos or trade dress. Don't imply endorsement.

## Third-party data
| Source | Terms summary | Our obligations |
|---|---|---|
| Scryfall | Free for Magic software and research under the Fan Content Policy. Cache ≥ 24 h. Don't overload the API. Don't paywall their data. | Use bulk files, send a polite User-Agent, cache, credit Scryfall. |
| MTGJSON | MIT License | Include their license notice if we redistribute derived files (we don't plan to). |
| EDHTop16 / tournament data | Check their API terms before use ([[cEDH Ground-Truth Data]]) | TBD |
| Moxfield / Archidekt | No official public API documentation found. Scraping may violate ToS. | Prefer user-pasted exports ([[Deck Import]]). |

## Prior-art code
- **phase.rs:** MIT/Apache, so it's compatible and we may reuse code with attribution.
- **XMage:** MIT (verified on GitHub, 2026-10-20). That's compatible, so reuse with attribution is allowed, though Java code rarely ports directly.
- **Forge:** **GPL-3.0**, which is incompatible with our permissive license. **Don't copy code or card scripts** from Forge. Studying it for ideas is fine.

## Related
[[Card Data Sources]] · [[ADR-0002 Licensing and Card Data Distribution]]
