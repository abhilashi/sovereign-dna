//! Deterministic, dependency-free PRNG for the federated prototype.
//! **[PROTOTYPE/EXPERIMENTAL]**
//!
//! We deliberately avoid pulling in the `rand` crate so the prototype adds **no
//! new dependencies** and every round is **exactly reproducible** from a seed —
//! reproducibility is a feature here (it is what makes a round manifest
//! content-addressable, §4.7).
//!
//! ⚠️ **Not cryptographically secure.** SplitMix64 is fine for simulating the
//! pairwise masks and calibrated noise of this *offline, in-process* prototype,
//! but a real deployment (§4.3/§4.4) must use a CSPRNG (e.g. ChaCha20) seeded
//! from OS entropy and an authenticated key-agreement for pairwise seeds. This
//! is called out in the PR as a gated/future item.

/// A tiny SplitMix64 generator — deterministic given its seed.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Next raw 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform f64 in `[0, 1)` with 53 bits of precision.
    pub fn next_f64(&mut self) -> f64 {
        // Top 53 bits → mantissa.
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// A standard-normal sample via the Box–Muller transform.
    pub fn next_gaussian(&mut self) -> f64 {
        // Guard u1 away from 0 so ln() is finite.
        let mut u1 = self.next_f64();
        if u1 < 1e-12 {
            u1 = 1e-12;
        }
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}

/// Derive a stable 64-bit seed from a label + two indices (order-insensitive on
/// the pair so peers `i` and `j` derive the *same* pairwise mask seed).
pub fn pair_seed(round_seed: u64, a: usize, b: usize) -> u64 {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let mut mix = SplitMix64::new(round_seed ^ 0xA5A5_5A5A_A5A5_5A5A);
    // Fold the ordered pair into the stream deterministically.
    mix.state = mix.state.wrapping_add((lo as u64).wrapping_mul(0x100_0000_01B3));
    let _ = mix.next_u64();
    mix.state = mix.state.wrapping_add((hi as u64).wrapping_mul(0x100_0000_01B3));
    mix.next_u64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_stream() {
        let mut a = SplitMix64::new(42);
        let mut b = SplitMix64::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn uniform_in_unit_interval() {
        let mut r = SplitMix64::new(7);
        for _ in 0..10_000 {
            let x = r.next_f64();
            assert!((0.0..1.0).contains(&x));
        }
    }

    #[test]
    fn gaussian_mean_and_std_are_sane() {
        let mut r = SplitMix64::new(123);
        let n = 200_000;
        let mut sum = 0.0;
        let mut sq = 0.0;
        for _ in 0..n {
            let x = r.next_gaussian();
            sum += x;
            sq += x * x;
        }
        let mean = sum / n as f64;
        let var = sq / n as f64 - mean * mean;
        assert!(mean.abs() < 0.02, "mean {mean} should be ~0");
        assert!((var - 1.0).abs() < 0.05, "var {var} should be ~1");
    }

    #[test]
    fn pair_seed_is_symmetric() {
        assert_eq!(pair_seed(99, 2, 5), pair_seed(99, 5, 2));
        assert_ne!(pair_seed(99, 2, 5), pair_seed(99, 2, 6));
    }
}
