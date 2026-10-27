//! Statistics for reports (M7 plan S10; `docs/11 Research/Statistics for
//! Multiplayer Simulation.md`). Inputs are integer counts and values in a
//! fixed order, and the bootstrap is seeded, so reports are bit-identical.

use kiln_engine::rng::{GameRng, splitmix64};

/// z for a two-sided 95% interval.
const Z95: f64 = 1.959_963_984_540_054;
/// Bootstrap resamples.
const RESAMPLES: usize = 1000;

/// The Wilson score interval for `k` successes in `n` trials (95%).
pub fn wilson(k: u64, n: u64) -> (f64, f64) {
    if n == 0 {
        return (0.0, 1.0);
    }
    let (all, none) = (k == n, k == 0);
    let (k, n) = (k as f64, n as f64);
    let p = k / n;
    let z2 = Z95 * Z95;
    let denom = 1.0 + z2 / n;
    let centre = (p + z2 / (2.0 * n)) / denom;
    let half = Z95 * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt() / denom;
    // The bounds are exactly 0 and 1 at the extremes (rounding aside).
    let lo = if none { 0.0 } else { (centre - half).max(0.0) };
    let hi = if all { 1.0 } else { (centre + half).min(1.0) };
    (lo, hi)
}

/// The value at quantile `q` (0..=1) of sorted `v`, by nearest rank.
pub fn quantile(sorted: &[u32], q: f64) -> u32 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (q * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

pub fn mean(v: &[u32]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.iter().map(|x| f64::from(*x)).sum::<f64>() / v.len() as f64
}

/// A 95% percentile-bootstrap interval for the median of `v`, seeded by `seed`.
pub fn median_ci(v: &[u32], seed: u64) -> (u32, u32) {
    if v.is_empty() {
        return (0, 0);
    }
    let mut rng = GameRng::new(splitmix64(seed ^ 0xB007_5747));
    let mut medians: Vec<u32> = Vec::with_capacity(RESAMPLES);
    let mut sample = vec![0u32; v.len()];
    for _ in 0..RESAMPLES {
        for s in &mut sample {
            *s = v[rng.index(v.len())];
        }
        sample.sort_unstable();
        medians.push(quantile(&sample, 0.5));
    }
    medians.sort_unstable();
    (quantile(&medians, 0.025), quantile(&medians, 0.975))
}

/// A 95% paired-bootstrap interval for the mean of `diffs` (A/B with common
/// random numbers: one difference per shared seed).
pub fn paired_mean_ci(diffs: &[i32], seed: u64) -> (f64, f64) {
    if diffs.is_empty() {
        return (0.0, 0.0);
    }
    let mut rng = GameRng::new(splitmix64(seed ^ 0xA_B0_07));
    let mut means: Vec<f64> = Vec::with_capacity(RESAMPLES);
    for _ in 0..RESAMPLES {
        let mut sum = 0i64;
        for _ in 0..diffs.len() {
            sum += i64::from(diffs[rng.index(diffs.len())]);
        }
        means.push(sum as f64 / diffs.len() as f64);
    }
    means.sort_by(f64::total_cmp);
    let at = |q: f64| means[((q * RESAMPLES as f64).ceil() as usize).clamp(1, RESAMPLES) - 1];
    (at(0.025), at(0.975))
}

/// Benjamini–Hochberg adjusted p-values (q-values), in input order.
pub fn benjamini_hochberg(p: &[f64]) -> Vec<f64> {
    let m = p.len();
    let mut order: Vec<usize> = (0..m).collect();
    order.sort_by(|a, b| p[*a].total_cmp(&p[*b]).then(a.cmp(b)));
    let mut q = vec![0.0; m];
    let mut running = 1.0f64;
    for (rank, &i) in order.iter().enumerate().rev() {
        let adj = (p[i] * m as f64 / (rank + 1) as f64).min(1.0);
        running = running.min(adj);
        q[i] = running;
    }
    q
}

/// Two-sided p-value of a two-proportion z-test (`k1/n1` vs `k2/n2`).
pub fn two_proportion_p(k1: u64, n1: u64, k2: u64, n2: u64) -> f64 {
    if n1 == 0 || n2 == 0 {
        return 1.0;
    }
    let (p1, p2) = (k1 as f64 / n1 as f64, k2 as f64 / n2 as f64);
    let pooled = (k1 + k2) as f64 / (n1 + n2) as f64;
    let se = (pooled * (1.0 - pooled) * (1.0 / n1 as f64 + 1.0 / n2 as f64)).sqrt();
    if se == 0.0 {
        return 1.0;
    }
    2.0 * (1.0 - normal_cdf(((p1 - p2) / se).abs()))
}

/// The standard normal CDF (Abramowitz–Stegun 7.1.26 via erf; error < 1.5e-7).
fn normal_cdf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.327_591_1 * x / std::f64::consts::SQRT_2);
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let erf = 1.0 - poly * (-(x * x) / 2.0).exp();
    0.5 * (1.0 + erf)
}

/// Rounds for output (4 decimal places).
pub fn r4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wilson_matches_reference_values() {
        // 25 of 100: the textbook interval is about [0.1755, 0.3430].
        let (lo, hi) = wilson(25, 100);
        assert!(
            (lo - 0.1755).abs() < 5e-4 && (hi - 0.3430).abs() < 5e-4,
            "{lo} {hi}"
        );
        // 1,250 of 5,000 (p = 0.25): half-width about 1.2 points.
        let (lo, hi) = wilson(1250, 5000);
        assert!(((hi - lo) / 2.0 - 0.012).abs() < 5e-4);
        assert_eq!(wilson(0, 0), (0.0, 1.0));
        assert!(wilson(0, 10).0 == 0.0 && wilson(10, 10).1 == 1.0);
    }

    #[test]
    fn quantiles_and_bootstrap() {
        let v: Vec<u32> = (1..=10).collect();
        assert_eq!(quantile(&v, 0.5), 5);
        assert_eq!(quantile(&v, 0.1), 1);
        assert_eq!(quantile(&v, 0.9), 9);
        let (lo, hi) = median_ci(&v, 1);
        assert!(lo <= 5 && hi >= 5 && lo >= 1 && hi <= 10);
        assert_eq!(median_ci(&v, 1), median_ci(&v, 1), "seeded");
    }

    #[test]
    fn bh_adjusts_in_input_order() {
        let q = benjamini_hochberg(&[0.01, 0.04, 0.03, 0.5]);
        assert!((q[0] - 0.04).abs() < 1e-12);
        assert!((q[1] - 0.0533).abs() < 1e-3);
        assert!((q[2] - 0.0533).abs() < 1e-3);
        assert!((q[3] - 0.5).abs() < 1e-12);
    }

    #[test]
    fn proportion_test_and_paired_ci() {
        assert!(two_proportion_p(500, 1000, 500, 1000) > 0.99);
        assert!(two_proportion_p(600, 1000, 500, 1000) < 1e-4);
        let diffs = vec![1, 0, 0, 1, 0, 0, -1, 0, 1, 0];
        let (lo, hi) = paired_mean_ci(&diffs, 3);
        assert!(lo <= 0.2 && hi >= 0.2);
    }
}
