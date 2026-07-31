//! Differential privacy: gradient clipping, calibrated Gaussian noise, and a
//! tracked global `(ε, δ)` budget (Phase 4.2). **[PROTOTYPE/EXPERIMENTAL]**
//!
//! This is the **load-bearing** part of the whole phase: the guarantee "we share
//! *learnings*, never DNA" is only meaningful if the learnings that leave a
//! device carry a *quantified, bounded* differential-privacy guarantee. So the
//! accounting here is done properly even though the model (`model.rs`) is a toy.
//!
//! ## Mechanism (DP-SGD style, per node, per round)
//! 1. **Clip** each node's local gradient to L2-norm ≤ `clip` (= sensitivity `C`).
//!    This bounds any single individual's influence on the shared update.
//! 2. **Add Gaussian noise** with standard deviation `σ = noise_multiplier · C`.
//!    The pair `(C, σ)` defines the Gaussian mechanism.
//!
//! Because every node participates in every round (cross-silo FL — **no**
//! Poisson subsampling), the per-round mechanism is a plain Gaussian mechanism
//! and there is **no privacy amplification by subsampling** to claim. We account
//! for it honestly with **Rényi Differential Privacy (RDP)**, which composes
//! additively across rounds, then convert to `(ε, δ)`-DP.
//!
//! ## RDP accounting (standard results)
//! * A Gaussian mechanism with noise multiplier `z = σ/C` is `(α, α / (2 z²))`-RDP
//!   for every order `α > 1` (Mironov, 2017).
//! * RDP **composes by summation**: after `T` rounds the accumulated RDP at order
//!   `α` is `Σ_t α / (2 z_t²)`.
//! * Convert RDP to `(ε, δ)`-DP with the tight bound (Canonne–Kamath–Steinke, 2020,
//!   as used in Opacus): for order `α > 1`,
//!   `ε(α) = ρ(α) + log((α−1)/α) − (log δ + log α)/(α−1)`,
//!   and report `ε = min_α ε(α)`.
//!
//! Sanity checks encoded as tests: ε grows with more rounds, shrinks with more
//! noise, and lands in the textbook ballpark for known parameters.

use serde::{Deserialize, Serialize};

use super::prng::SplitMix64;

/// The L2 norm of a vector.
pub fn l2_norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// Clip `grad` in place so its L2 norm is at most `clip` (the sensitivity `C`).
/// Returns the scaling factor applied (`1.0` if it was already within bound).
pub fn clip_l2(grad: &mut [f64], clip: f64) -> f64 {
    assert!(clip > 0.0, "clip norm must be positive");
    let norm = l2_norm(grad);
    if norm <= clip || norm == 0.0 {
        return 1.0;
    }
    let scale = clip / norm;
    for g in grad.iter_mut() {
        *g *= scale;
    }
    scale
}

/// A Gaussian mechanism configuration for one round.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GaussianMechanism {
    /// The L2 clip / sensitivity `C`.
    pub clip: f64,
    /// The noise multiplier `z = σ / C`. Higher = more privacy, less utility.
    pub noise_multiplier: f64,
}

impl GaussianMechanism {
    pub fn new(clip: f64, noise_multiplier: f64) -> Self {
        assert!(clip > 0.0 && noise_multiplier > 0.0);
        Self {
            clip,
            noise_multiplier,
        }
    }

    /// The absolute noise standard deviation `σ = z · C`.
    pub fn sigma(&self) -> f64 {
        self.noise_multiplier * self.clip
    }

    /// Add calibrated Gaussian noise (in place) using a deterministic PRNG.
    pub fn add_noise(&self, grad: &mut [f64], rng: &mut SplitMix64) {
        let sigma = self.sigma();
        for g in grad.iter_mut() {
            *g += sigma * rng.next_gaussian();
        }
    }

    /// Convenience: clip then noise a fresh copy of `grad`.
    pub fn privatize(&self, grad: &[f64], rng: &mut SplitMix64) -> Vec<f64> {
        let mut out = grad.to_vec();
        clip_l2(&mut out, self.clip);
        self.add_noise(&mut out, rng);
        out
    }
}

/// A Rényi-DP privacy accountant tracking a **global** budget across rounds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyAccountant {
    /// RDP orders α to track.
    orders: Vec<f64>,
    /// Accumulated RDP `ρ(α)` at each order.
    rdp: Vec<f64>,
    /// Number of composed rounds (for reporting).
    pub rounds: u64,
}

impl Default for PrivacyAccountant {
    fn default() -> Self {
        Self::new()
    }
}

impl PrivacyAccountant {
    /// A default order grid covering the useful range for genomics-scale ε.
    pub fn new() -> Self {
        let orders = vec![
            1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 16.0, 24.0, 32.0, 48.0,
            64.0, 128.0, 256.0,
        ];
        let n = orders.len();
        Self {
            orders,
            rdp: vec![0.0; n],
            rounds: 0,
        }
    }

    /// Compose one round of the Gaussian mechanism with noise multiplier `z`.
    /// A non-subsampled Gaussian mechanism is `(α, α / (2 z²))`-RDP.
    pub fn compose_gaussian(&mut self, noise_multiplier: f64) {
        assert!(noise_multiplier > 0.0);
        let z2 = noise_multiplier * noise_multiplier;
        for (r, &a) in self.rdp.iter_mut().zip(&self.orders) {
            *r += a / (2.0 * z2);
        }
        self.rounds += 1;
    }

    /// Current `ε` at a target `δ` — the minimum over tracked RDP orders using
    /// the Canonne–Kamath–Steinke conversion. Returns `0.0` before any round.
    pub fn epsilon(&self, delta: f64) -> f64 {
        assert!(delta > 0.0 && delta < 1.0, "delta must be in (0,1)");
        if self.rounds == 0 {
            return 0.0;
        }
        let mut best = f64::INFINITY;
        for (&rho, &a) in self.rdp.iter().zip(&self.orders) {
            if a <= 1.0 {
                continue;
            }
            // ε(α) = ρ + log((α−1)/α) − (log δ + log α)/(α−1)
            let eps = rho + ((a - 1.0) / a).ln() - (delta.ln() + a.ln()) / (a - 1.0);
            if eps.is_finite() && eps < best {
                best = eps;
            }
        }
        best.max(0.0)
    }

    /// A printable one-line budget report.
    pub fn report(&self, delta: f64) -> String {
        format!(
            "privacy budget after {} round(s): (ε = {:.4}, δ = {:.1e})",
            self.rounds,
            self.epsilon(delta),
            delta
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_scales_down_only_when_over_bound() {
        let mut g = vec![3.0, 4.0]; // norm 5
        let s = clip_l2(&mut g, 1.0);
        assert!((s - 0.2).abs() < 1e-12);
        assert!((l2_norm(&g) - 1.0).abs() < 1e-12);

        let mut small = vec![0.1, 0.1];
        let s2 = clip_l2(&mut small, 1.0);
        assert_eq!(s2, 1.0);
        assert_eq!(small, vec![0.1, 0.1]);
    }

    #[test]
    fn noise_has_expected_scale() {
        let mech = GaussianMechanism::new(1.0, 2.0); // sigma = 2
        let mut rng = SplitMix64::new(5);
        let n = 100_000usize;
        let mut g = vec![0.0; n];
        mech.add_noise(&mut g, &mut rng);
        let mean = g.iter().sum::<f64>() / n as f64;
        let var = g.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
        assert!(mean.abs() < 0.05, "mean {mean}");
        assert!((var.sqrt() - 2.0).abs() < 0.05, "std {}", var.sqrt());
    }

    #[test]
    fn privatize_is_deterministic_for_a_seed() {
        let mech = GaussianMechanism::new(1.0, 1.0);
        let g = vec![10.0, -20.0, 5.0];
        let a = mech.privatize(&g, &mut SplitMix64::new(1));
        let b = mech.privatize(&g, &mut SplitMix64::new(1));
        assert_eq!(a, b);
        // The clipped signal is bounded; noise perturbs it away from the clip.
        assert_ne!(a, mech.privatize(&g, &mut SplitMix64::new(2)));
    }

    #[test]
    fn epsilon_is_zero_before_any_round() {
        let acc = PrivacyAccountant::new();
        assert_eq!(acc.epsilon(1e-5), 0.0);
    }

    #[test]
    fn epsilon_grows_with_rounds_and_shrinks_with_noise() {
        let delta = 1e-5;

        let mut low_noise = PrivacyAccountant::new();
        low_noise.compose_gaussian(1.0);
        let e1 = low_noise.epsilon(delta);
        low_noise.compose_gaussian(1.0);
        let e2 = low_noise.epsilon(delta);
        assert!(e2 > e1, "more rounds must spend more budget: {e1} -> {e2}");

        // More noise at the same round count → smaller ε.
        let mut high_noise = PrivacyAccountant::new();
        high_noise.compose_gaussian(4.0);
        assert!(
            high_noise.epsilon(delta) < e1,
            "more noise must cost less budget"
        );
    }

    #[test]
    fn epsilon_matches_textbook_ballpark() {
        // z = 1, single Gaussian round, δ = 1e-5 → ε ≈ 5 (a few units), never absurd.
        let mut acc = PrivacyAccountant::new();
        acc.compose_gaussian(1.0);
        let eps = acc.epsilon(1e-5);
        assert!(eps > 3.0 && eps < 7.0, "ε = {eps} outside expected band");

        // Composition scales sub-linearly (√T behaviour), not linearly.
        let mut ten = PrivacyAccountant::new();
        for _ in 0..10 {
            ten.compose_gaussian(1.0);
        }
        let eps10 = ten.epsilon(1e-5);
        assert!(eps10 > eps, "10 rounds > 1 round");
        assert!(
            eps10 < 10.0 * eps,
            "advanced composition must beat linear: {eps10} vs {}",
            10.0 * eps
        );
    }
}
