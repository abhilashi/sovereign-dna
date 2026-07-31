//! Secure aggregation via pairwise additive masking (Phase 4.3, simulated).
//! **[PROTOTYPE/EXPERIMENTAL]**
//!
//! Goal: let an aggregator learn the **sum** of the nodes' (already clipped +
//! DP-noised) updates **without seeing any individual update in the clear** —
//! removing the trusted-aggregator requirement.
//!
//! ## Bonawitz-style pairwise masks
//! For every unordered pair of nodes `{i, j}` we derive a shared random mask
//! vector `s_ij` from a common seed (a real system establishes this via
//! authenticated Diffie–Hellman key agreement; here it is derived from a round
//! seed — see `prng::pair_seed` and the security note below). Node `i` adds
//! `+s_ij` for every peer `j > i` and `−s_ij` for every peer `j < i`:
//!
//! ```text
//! mask_i = Σ_{j > i} s_ij  −  Σ_{j < i} s_ij
//! masked_i = update_i + mask_i
//! ```
//!
//! Summing all masked shares, every `s_ij` appears once with `+` and once with
//! `−`, so **the masks cancel exactly** and `Σ_i masked_i = Σ_i update_i`. Any
//! *single* `masked_i` is one-time-padded by fresh randomness, so the aggregator
//! (and any observer) sees only a value indistinguishable from random — never the
//! individual clear update.
//!
//! We generate the shared `s_ij` bit-identically on both sides, so their f64
//! values cancel **exactly** (no quantisation error) in this offline simulation.
//!
//! ## What this prototype does *not* do (gated / future)
//! * No real key agreement / secure channels (§4.4) — pairwise seeds are derived
//!   locally for the in-process simulation.
//! * **No dropout recovery.** Real Bonawitz secure-agg uses Shamir secret shares
//!   of each node's PRG seed so the surviving nodes can reconstruct the masks of
//!   nodes that drop mid-round. Here we assume all simulated nodes complete.
//! * No malicious-server / active-adversary hardening.

use super::prng::{pair_seed, SplitMix64};

/// Derive the shared pairwise mask vector `s_ij` (length `len`) for nodes `a`
/// and `b`. Symmetric in `(a, b)` so both peers derive the same vector.
fn pairwise_mask(round_seed: u64, a: usize, b: usize, len: usize) -> Vec<f64> {
    let seed = pair_seed(round_seed, a, b);
    let mut rng = SplitMix64::new(seed);
    // Uniform in [-1, 1) — bounded so masked shares stay finite; the exact range
    // is irrelevant to correctness because masks cancel.
    (0..len).map(|_| rng.next_f64() * 2.0 - 1.0).collect()
}

/// The additive mask a node applies to its own update:
/// `mask_i = Σ_{j>i} s_ij − Σ_{j<i} s_ij`.
pub fn mask_for_node(node_i: usize, num_nodes: usize, round_seed: u64, len: usize) -> Vec<f64> {
    let mut mask = vec![0.0; len];
    for j in 0..num_nodes {
        if j == node_i {
            continue;
        }
        let s = pairwise_mask(round_seed, node_i, j, len);
        let sign = if j > node_i { 1.0 } else { -1.0 };
        for (m, sv) in mask.iter_mut().zip(&s) {
            *m += sign * sv;
        }
    }
    mask
}

/// A node's masked share = its (private) update plus its pairwise mask.
/// This is the only per-node value that crosses the boundary in a real system.
pub fn masked_share(
    update: &[f64],
    node_i: usize,
    num_nodes: usize,
    round_seed: u64,
) -> Vec<f64> {
    let mask = mask_for_node(node_i, num_nodes, round_seed, update.len());
    update.iter().zip(&mask).map(|(u, m)| u + m).collect()
}

/// The aggregator's view: sum the masked shares element-wise. Because pairwise
/// masks cancel, this equals the true sum of the underlying updates — yet the
/// aggregator only ever handled masked (random-looking) shares.
pub fn aggregate(masked_shares: &[Vec<f64>]) -> Vec<f64> {
    let len = masked_shares.first().map(|s| s.len()).unwrap_or(0);
    let mut sum = vec![0.0; len];
    for share in masked_shares {
        assert_eq!(share.len(), len, "all shares must have equal length");
        for (acc, v) in sum.iter_mut().zip(share) {
            *acc += v;
        }
    }
    sum
}

/// Full simulated secure-aggregation: produce every node's masked share for
/// `updates` then aggregate them. Returns `(masked_shares, aggregate_sum)`.
pub fn secure_sum(updates: &[Vec<f64>], round_seed: u64) -> (Vec<Vec<f64>>, Vec<f64>) {
    let n = updates.len();
    let masked: Vec<Vec<f64>> = updates
        .iter()
        .enumerate()
        .map(|(i, u)| masked_share(u, i, n, round_seed))
        .collect();
    let agg = aggregate(&masked);
    (masked, agg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain_sum(updates: &[Vec<f64>]) -> Vec<f64> {
        let len = updates[0].len();
        let mut s = vec![0.0; len];
        for u in updates {
            for (acc, v) in s.iter_mut().zip(u) {
                *acc += v;
            }
        }
        s
    }

    #[test]
    fn masks_cancel_and_sum_is_exact() {
        let updates = vec![
            vec![1.0, 2.0, 3.0],
            vec![-4.0, 0.5, 10.0],
            vec![7.0, -2.0, 0.0],
            vec![0.25, 0.25, 0.25],
        ];
        let (_, agg) = secure_sum(&updates, 0xDEAD_BEEF);
        let truth = plain_sum(&updates);
        for (a, t) in agg.iter().zip(&truth) {
            assert!((a - t).abs() < 1e-9, "agg {a} vs truth {t}");
        }
    }

    #[test]
    fn single_share_hides_the_individual_update() {
        let updates = vec![vec![100.0, -100.0], vec![1.0, 1.0], vec![0.0, 0.0]];
        let (masked, _) = secure_sum(&updates, 42);
        // Node 0's masked share must not equal its clear update.
        assert_ne!(masked[0], updates[0]);
        // And the mask must be substantial (not a near-zero perturbation).
        let diff = (masked[0][0] - updates[0][0]).abs() + (masked[0][1] - updates[0][1]).abs();
        assert!(diff > 1e-6, "masked share should differ meaningfully");
    }

    #[test]
    fn no_single_view_partial_aggregate_leaks_nothing() {
        // If the aggregator sums only N-1 shares (drops one), the uncancelled
        // masks mean it does NOT recover the partial true sum → it cannot isolate
        // the missing node's update.
        let updates = vec![vec![5.0, 6.0], vec![7.0, 8.0], vec![9.0, 10.0]];
        let (masked, _) = secure_sum(&updates, 7);
        let partial = aggregate(&masked[..2].to_vec());
        let partial_truth = plain_sum(&updates[..2].to_vec());
        // The two must differ: leftover masks from the excluded node remain.
        let differ = partial
            .iter()
            .zip(&partial_truth)
            .any(|(a, b)| (a - b).abs() > 1e-6);
        assert!(differ, "partial aggregate must not equal partial true sum");
    }

    #[test]
    fn all_masks_sum_to_zero() {
        let n = 5;
        let len = 4;
        let seed = 999;
        let mut total = vec![0.0; len];
        for i in 0..n {
            let m = mask_for_node(i, n, seed, len);
            for (t, v) in total.iter_mut().zip(&m) {
                *t += v;
            }
        }
        for t in total {
            assert!(t.abs() < 1e-9, "masks must cancel to zero, got {t}");
        }
    }

    #[test]
    fn single_node_has_no_mask() {
        let m = mask_for_node(0, 1, 3, 3);
        assert_eq!(m, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn secure_agg_composes_with_dp_noise() {
        // Sum of DP-noised updates equals plain sum of the same noised updates —
        // secure-agg is transparent to whatever the updates already contain.
        use super::super::dp::GaussianMechanism;
        use super::super::prng::SplitMix64;
        let mech = GaussianMechanism::new(1.0, 0.5);
        let raw = [vec![0.8, -0.6], vec![0.3, 0.4], vec![-0.5, 0.1]];
        let noised: Vec<Vec<f64>> = raw
            .iter()
            .enumerate()
            .map(|(i, g)| mech.privatize(g, &mut SplitMix64::new(1000 + i as u64)))
            .collect();
        let (_, agg) = secure_sum(&noised, 55);
        let truth = plain_sum(&noised);
        for (a, t) in agg.iter().zip(&truth) {
            assert!((a - t).abs() < 1e-9);
        }
    }
}
