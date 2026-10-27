//! Game randomness (`docs/03 Architecture/Subsystems/Randomness.md`).
//!
//! One ChaCha8 stream per game. Sampling (bounded integers, shuffles) is
//! implemented here rather than borrowed from `rand`, so a dependency upgrade
//! can never change which cards a seed shuffles (ADR-0005).

use rand_chacha::ChaCha8Rng;
use rand_core::{Rng, SeedableRng};

/// The game's random stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameRng {
    inner: ChaCha8Rng,
    /// Number of 64-bit draws taken, part of the state hash.
    draws: u64,
}

impl GameRng {
    pub fn new(seed: u64) -> Self {
        Self {
            inner: ChaCha8Rng::seed_from_u64(seed),
            draws: 0,
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.draws += 1;
        self.inner.next_u64()
    }

    pub fn draws(&self) -> u64 {
        self.draws
    }

    /// Uniform integer in `0..n` (Lemire's nearly-divisionless method, unbiased).
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "below(0)");
        let threshold = n.wrapping_neg() % n;
        loop {
            let m = u128::from(self.next_u64()) * u128::from(n);
            if (m as u64) >= threshold {
                return (m >> 64) as u64;
            }
        }
    }

    /// Uniform index into a slice of length `len`.
    pub fn index(&mut self, len: usize) -> usize {
        usize::try_from(self.below(len as u64)).unwrap_or(0)
    }

    /// Fisher–Yates shuffle (CR 701.24a: a random order).
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.index(i + 1);
            items.swap(i, j);
        }
    }
}

/// SplitMix64, used to derive independent seeds (per game, per seat) from an
/// experiment seed without consuming a shared stream (ADR-0005).
pub fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Seed for game `index` of an experiment.
pub fn game_seed(experiment_seed: u64, index: u64) -> u64 {
    splitmix64(splitmix64(experiment_seed) ^ splitmix64(index.wrapping_add(0x5EED)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_stream() {
        let mut a = GameRng::new(42);
        let mut b = GameRng::new(42);
        let xs: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
        let ys: Vec<u64> = (0..8).map(|_| b.next_u64()).collect();
        assert_eq!(xs, ys);
        assert_ne!(GameRng::new(43).next_u64(), xs[0]);
    }

    #[test]
    fn below_is_in_range_and_roughly_uniform() {
        let mut r = GameRng::new(7);
        let mut counts = [0u32; 6];
        for _ in 0..60_000 {
            counts[r.index(6)] += 1;
        }
        assert!(
            counts.iter().all(|&c| (9_000..11_000).contains(&c)),
            "{counts:?}"
        );
    }

    #[test]
    fn shuffle_is_a_permutation() {
        let mut r = GameRng::new(1);
        let mut v: Vec<u32> = (0..100).collect();
        r.shuffle(&mut v);
        let mut sorted = v.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..100).collect::<Vec<_>>());
        assert_ne!(v, sorted);
    }

    #[test]
    fn seeds_differ_per_game() {
        assert_ne!(game_seed(1, 0), game_seed(1, 1));
        assert_eq!(game_seed(1, 5), game_seed(1, 5));
    }
}
