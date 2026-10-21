---
type: process
status: draft
tags: [rules-map, process, maintenance]
updated: 2026-10-20
---

# CR Update Process

Wizards publishes a new Comprehensive Rules with most set releases, usually several times a year. Banned lists and Oracle errata change on their own schedules. NFR-M1 requires integrating a new CR within 2 weeks.

## Trigger
- A new CR file appears on the [official rules page](https://magic.wizards.com/en/rules), at a URL of the form `media.wizards.com/YYYY/downloads/MagicCompRules YYYYMMDD.txt`.
- A new set releases (new cards and new mechanics).
- A banned and restricted announcement.

## Steps
1. **Fetch** the new CR into `_external/` (see `CLAUDE.md`).
2. **Diff:** the CR parser (planned in M1, [[CR Structure]]) produces a rule-level diff: added, removed, changed, renumbered. Renumbering is common, because new keyword abilities get inserted.
3. **Re-validate citations:** CI fails on any `CR x.y` reference that no longer exists or whose text changed materially. Each failure gets triaged.
4. **Triage changed rules** into the owning subsystems ([[CR to Subsystem Map]]). Create an issue per behavioural change.
5. **New mechanics:** add keyword entries to the coverage tracker and to the card-definition backlog.
6. **Card data refresh:** pull a new Scryfall/MTGJSON snapshot, run ingestion, and produce a coverage diff of new, changed (errata) and unchanged cards. Errata'd cards drop to `parsed` until re-verified.
7. **Golden replays:** run them. Divergences are expected for rules changes and must be re-blessed with a link to the change ([[Golden Replays]]).
8. **Bump** `cr_version` in the engine config, the reports, and [[CR Structure]].

## Notable recent rules changes to keep in mind
- **Damage assignment order was removed.** Blocked creatures now divide damage freely (CR 510.1c) and blockers likewise (510.1d).
- **2HG combat** now uses the shared-team-turns combat rules (CR 810.7 notes "This is a change from previous rules").
- New card-type sections: 720 Omen, 721 Station, 722 Preparation cards.

## Related
[[CR Structure]] · [[Ingestion Pipeline]] · [[Golden Replays]]
