---
type: subsystem
status: draft
cr: ["704"]
impl_status: not-started
milestone: M2
tags: [subsystem, sba]
updated: 2026-10-20
---

# State-Based Actions

## Purpose
This subsystem checks and performs state-based actions whenever a player would receive priority (CR 704.3, 117.5).

## CR sections
704 (704.5a–704.5z in the 2026-06-19 CR), plus variant SBAs in 704.6, such as commander damage (704.6c).

## Responsibilities
- Check **all** applicable SBAs, perform them **simultaneously as one event**, and repeat until none apply. Then put triggers on the stack (CR 704.3).
- Implement the full list, including newer ones:
  - Roles (704.5y)
  - speed / start your engines (704.5z)
  - battles (704.5v–x)
  - Sagas (704.5s)
  - dungeons (704.5t)
  - space sculptor (704.5u)
- Variant SBAs: commander damage (704.6c), 2HG life and poison (see [[Two-Headed Giant]]).
- Commander to the command zone from graveyard or exile (CR 903.9a) is an SBA with an owner **choice**.

## Decisions raised
- Legend rule: choose which to keep (CR 704.5j).
- Commander zone choice (CR 903.9a).
- Various "chooses one" SBAs (Roles, Siege protector).

## Performance
SBA checks run before *every* priority, so they need to be incremental. Track "dirty" flags: life changed, toughness or damage changed, a legendary entered, counters changed, and so on. Only re-check the affected categories.

## Edge cases
- A player at 0 life and a player who drew from an empty library both lose simultaneously. In multiplayer the game continues (CR 800.4).
- 0 toughness vs. lethal damage vs. deathtouch: different SBAs with different regeneration interactions (CR 704.5f–h).
- The +1/+1 and -1/-1 counter annihilation (704.5q).
- All remaining players losing simultaneously means the game is a draw (CR 104.4a).

## Test scenarios
- Thassa's Oracle with an empty library: the win comes from the ETB trigger's resolution, not from an SBA, so test the ordering against a simultaneous empty-library draw.
- Walking Ballista at 0 counters dies to the 0-toughness SBA.

## Related
[[Priority & the Stack]] · [[Game End, Restarts & Subgames]] · [[Commander]]
