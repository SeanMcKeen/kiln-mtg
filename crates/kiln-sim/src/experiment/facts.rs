//! Per-seat facts from a finished game's event log (Metrics Catalog
//! `per_card`, `interaction`, `mana`; M7 plan S9).

use std::collections::BTreeMap;

use kiln_engine::ir::Effect;
use kiln_engine::{CardPool, DefId, Event, Game, ZoneId};
use kiln_protocol::record::{CardFact, SeatCards, SeatInteraction, SeatMana};
use kiln_types::ObjectId;

/// What to extract.
#[derive(Debug, Clone, Copy)]
pub struct Want<'a> {
    /// Seats whose cards are tracked (`per_card`); empty = none.
    pub card_seats: &'a [bool],
    pub interaction: bool,
    pub mana: bool,
}

#[derive(Debug, Default)]
pub struct Facts {
    pub cards: Option<Vec<SeatCards>>,
    pub interaction: Option<Vec<SeatInteraction>>,
    pub mana: Option<Vec<SeatMana>>,
}

/// Whether a spell counters something (Counterspell, Force of Will, …).
fn is_counterspell(pool: &CardPool, def: DefId) -> bool {
    pool.get(def).spell().is_some_and(|s| {
        let mut found = false;
        s.effect.walk(&mut |e| {
            found |= matches!(e, Effect::Counter(_) | Effect::CounterAndExile(_));
        });
        found
    })
}

/// Walks the events. `hands` is each seat's opening hand (turn 0);
/// `commanders` are left out of card facts.
pub fn extract(
    game: &Game,
    hands: &[Vec<DefId>],
    commanders: &[Vec<DefId>],
    want: Want<'_>,
) -> Facts {
    let pool = game.card_pool();
    let n = game.player_count();
    let tracked = |seat: usize| want.card_seats.get(seat).copied().unwrap_or(false);
    // seat → card → (seen turn, cast turn)
    let mut cards: Vec<BTreeMap<DefId, (u32, Option<u32>)>> = vec![BTreeMap::new(); n];
    for (seat, hand) in hands.iter().enumerate() {
        if tracked(seat) {
            for d in hand {
                cards[seat].entry(*d).or_insert((0, None));
            }
        }
    }
    let mut inter = vec![SeatInteraction::default(); n];
    let mut mana = vec![SeatMana::default(); n];
    let mut objects: BTreeMap<ObjectId, (DefId, usize)> = BTreeMap::new();
    let mut turn = 0u32;
    let mut active: Option<usize> = None;
    let mut own_turns = vec![0u32; n];
    let mut landed = false;
    let close_turn = |active: Option<usize>, landed: bool, own: &[u32], mana: &mut [SeatMana]| {
        if let Some(a) = active
            && own[a] <= 5
            && !landed
        {
            mana[a].missed_land_drops += 1;
        }
    };
    let cast =
        |seat: usize, def: DefId, turn: u32, cards: &mut [BTreeMap<DefId, (u32, Option<u32>)>]| {
            if tracked(seat) && !pool.get(def).token && !commanders[seat].contains(&def) {
                let e = cards[seat].entry(def).or_insert((turn, None));
                e.1.get_or_insert(turn);
            }
        };
    for e in game.events() {
        match e {
            Event::TurnBegan { number, active: a } => {
                close_turn(active, landed, &own_turns, &mut mana);
                turn = *number;
                let a = a.0 as usize;
                active = Some(a);
                landed = false;
                own_turns[a] += 1;
                if own_turns[a] <= 5 {
                    mana[a].early_turns += 1;
                }
            }
            Event::ZoneChange {
                def,
                controller,
                to,
                new,
                ..
            } => {
                objects.insert(*new, (*def, controller.0 as usize));
                if let ZoneId::Hand(p) = to {
                    let seat = p.0 as usize;
                    if turn >= 1
                        && tracked(seat)
                        && !pool.get(*def).token
                        && !commanders[seat].contains(def)
                    {
                        cards[seat].entry(*def).or_insert((turn, None));
                    }
                }
            }
            Event::SpellCast {
                player,
                object,
                def,
            } => {
                let seat = player.0 as usize;
                objects.insert(*object, (*def, seat));
                cast(seat, *def, turn, &mut cards);
                if want.interaction && is_counterspell(pool, *def) {
                    inter[seat].counterspells += 1;
                }
            }
            Event::LandPlayed { player, object } => {
                let seat = player.0 as usize;
                mana[seat].lands_played += 1;
                if active == Some(seat) {
                    landed = true;
                }
                if let Some((def, _)) = objects.get(object).copied() {
                    cast(seat, def, turn, &mut cards);
                }
            }
            Event::Countered { object } => {
                if let Some((_, seat)) = objects.get(object) {
                    inter[*seat].countered += 1;
                }
            }
            Event::BecameTarget {
                target, controller, ..
            } => {
                if let Some((_, owner)) = objects.get(target)
                    && *owner != controller.0 as usize
                {
                    inter[controller.0 as usize].targeted_opponents += 1;
                }
            }
            Event::ManaAdded { player, .. } => {
                mana[player.0 as usize].mana_produced += 1;
            }
            _ => {}
        }
    }
    close_turn(active, landed, &own_turns, &mut mana);
    Facts {
        cards: want.card_seats.iter().any(|t| *t).then(|| {
            cards
                .into_iter()
                .enumerate()
                .filter(|(seat, _)| tracked(*seat))
                .map(|(seat, m)| SeatCards {
                    seat,
                    cards: m
                        .into_iter()
                        .map(|(def, (seen, cast))| CardFact {
                            name: pool.get(def).name.clone(),
                            seen,
                            cast,
                        })
                        .collect(),
                })
                .collect()
        }),
        interaction: want.interaction.then_some(inter),
        mana: want.mana.then_some(mana),
    }
}
