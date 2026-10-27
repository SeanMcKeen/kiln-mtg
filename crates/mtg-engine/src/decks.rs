//! Synthetic test decks built from [`CardPool::test_set`] for fuzzing,
//! benchmarks and examples. Each deck is two colors, 40% lands.

use crate::card::{CardPool, DefId};

/// The two-color pairs used by [`test_deck`].
pub const PAIRS: [(&str, &str); 5] = [
    ("Forest", "Mountain"),
    ("Island", "Plains"),
    ("Swamp", "Forest"),
    ("Mountain", "Plains"),
    ("Island", "Swamp"),
];

fn color_of(land: &str) -> char {
    match land {
        "Plains" => 'W',
        "Island" => 'U',
        "Swamp" => 'B',
        "Mountain" => 'R',
        _ => 'G',
    }
}

/// A deck of `size` cards for pair `pair % 5` from the test set. Spells whose
/// colors fit the pair are included round-robin; lands fill 40%.
pub fn test_deck(pool: &CardPool, pair: usize, size: usize) -> Vec<DefId> {
    let (a, b) = PAIRS[pair % PAIRS.len()];
    let (ca, cb) = (color_of(a), color_of(b));
    let spells: Vec<DefId> = pool
        .iter()
        .filter(|(_, d)| d.mana_cost.is_some())
        .filter(|(_, d)| {
            d.colors.iter().all(|c| {
                let l = c.letter();
                l == ca || l == cb
            })
        })
        .map(|(id, _)| id)
        .collect();
    let land_a = pool.by_name(&format!("Test {a}"));
    let land_b = pool.by_name(&format!("Test {b}"));
    let lands = size * 2 / 5;
    let mut deck = Vec::with_capacity(size);
    for i in 0..lands {
        if let Some(l) = if i % 2 == 0 { land_a } else { land_b } {
            deck.push(l);
        }
    }
    let mut i = 0;
    while deck.len() < size && !spells.is_empty() {
        deck.push(spells[i % spells.len()]);
        i += 1;
    }
    deck
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decks_have_the_right_shape() {
        let pool = CardPool::test_set();
        for pair in 0..5 {
            let d = test_deck(&pool, pair, 60);
            assert_eq!(d.len(), 60);
            assert_eq!(
                d.iter()
                    .filter(|x| pool.get(**x).mana_cost.is_none())
                    .count(),
                24
            );
        }
    }
}
