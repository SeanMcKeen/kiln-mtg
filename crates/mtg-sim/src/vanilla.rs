//! Vanilla card pools and decks for the M2 fuzz runs.
//!
//! Real cards enter the M2 engine only if `CardDef::from_record` accepts them
//! (basic-type lands, vanilla and French-vanilla creatures). Everything else
//! is excluded, never treated as a blank card (Design Principle 9).

use mtg_core::{CardType, Colors};
use mtg_data::{CardDb, Legality};
use mtg_engine::rng::GameRng;
use mtg_engine::{CardDef, CardPool, DefId};

/// Builds a pool from the card DB's Commander-legal cards that the M2 IR can
/// express. Returns the pool and how many candidate cards were rejected.
pub fn vanilla_pool(db: &CardDb) -> (CardPool, usize) {
    let mut pool = CardPool::new();
    let mut rejected = 0;
    for rec in db.cards() {
        if !rec.layout.is_deck_card() || rec.legality("commander") != Legality::Legal {
            continue;
        }
        let t = &rec.front().type_line;
        if !(t.has_type(CardType::Creature) || t.has_type(CardType::Land)) {
            continue;
        }
        match CardDef::from_record(rec) {
            Ok(def) => {
                pool.add(def);
            }
            Err(_) => rejected += 1,
        }
    }
    (pool, rejected)
}

/// A random two-color deck of `size` cards: creatures whose colors fit the
/// pair, and 40% basic-type lands producing those colors. Deterministic in `seed`.
pub fn random_deck(pool: &CardPool, seed: u64, size: usize) -> Vec<DefId> {
    let mut rng = GameRng::new(seed);
    let a = mtg_core::Color::ALL[rng.index(5)];
    let mut b = mtg_core::Color::ALL[rng.index(5)];
    if b == a {
        b = mtg_core::Color::ALL[(a as usize + 1) % 5];
    }
    let pair = Colors::single(a).union(Colors::single(b));
    let mut creatures: Vec<DefId> = Vec::new();
    let mut lands: Vec<DefId> = Vec::new();
    for (id, def) in pool.iter() {
        if def.is(CardType::Land) {
            let produces: Colors = def
                .mana_abilities
                .iter()
                .filter_map(|m| mtg_core::Color::ALL.get(m.single_type()?.index()).copied())
                .collect();
            if !def.mana_abilities.is_empty() && produces.is_subset_of(pair) {
                lands.push(id);
            }
        } else if def.colors.is_subset_of(pair) && def.mana_cost.is_some() {
            creatures.push(id);
        }
    }
    let mut deck = Vec::with_capacity(size);
    let n_lands = size * 2 / 5;
    for _ in 0..n_lands {
        if !lands.is_empty() {
            deck.push(lands[rng.index(lands.len())]);
        }
    }
    while deck.len() < size && !creatures.is_empty() {
        deck.push(creatures[rng.index(creatures.len())]);
    }
    deck
}

/// Whether a definition is usable for random decks (sanity helper for tests).
pub fn is_vanilla_creature(def: &CardDef) -> bool {
    def.is(CardType::Creature) && def.spell().is_none()
}
