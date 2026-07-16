//! Benchmark-gated federated round with a reproducible, content-addressed
//! manifest (Phase 4.7). **[PROTOTYPE/EXPERIMENTAL]**
//!
//! A round is only **promoted** if the aggregated candidate model beats the
//! prior model on a held-out **synthetic** benchmark. This is the honest version
//! of "self-improving without a central server": each round is DP-bounded,
//! secure-aggregated, evaluated against fixed held-out truth, and recorded in a
//! reproducible manifest that is content-addressed by hash. A poisoned or merely
//! unhelpful round fails the gate and is discarded — the prior model stands.
//!
//! ## One round
//! 1. Each simulated node computes a local gradient (`model.rs`).
//! 2. Each gradient is **clipped + DP-noised** (`dp.rs`) with the round's
//!    Gaussian mechanism; the privacy accountant composes one Gaussian round.
//! 3. The noised gradients are **secure-aggregated** (`secure_agg.rs`) so no
//!    single party sees an individual update.
//! 4. Candidate model = `prior − lr · (aggregate / num_nodes)` (FedSGD step).
//! 5. **Gate:** evaluate prior vs candidate MSE on the held-out benchmark;
//!    promote iff the candidate is strictly better.
//! 6. Emit a `RoundManifest` (content-addressed by SHA-256 of its canonical JSON).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::dp::{GaussianMechanism, PrivacyAccountant};
use super::model::{LocalExample, LocalNode, Model};
use super::prng::SplitMix64;
use super::secure_agg;

/// Configuration for one federated round.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundConfig {
    /// L2 clip / sensitivity `C`.
    pub clip: f64,
    /// Noise multiplier `z = σ / C`.
    pub noise_multiplier: f64,
    /// FedSGD learning rate.
    pub learning_rate: f64,
    /// Target `δ` for ε reporting.
    pub delta: f64,
    /// Deterministic seed for this round (noise + masks) → reproducible manifest.
    pub round_seed: u64,
}

impl RoundConfig {
    pub fn mechanism(&self) -> GaussianMechanism {
        GaussianMechanism::new(self.clip, self.noise_multiplier)
    }
}

/// A reproducible, content-addressed record of one round. All fields are public,
/// aggregate metadata — there is **no field capable of holding a genotype or a
/// per-individual gradient**.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundManifest {
    pub prototype: bool,
    pub round: u64,
    pub num_nodes: usize,
    pub num_variants: usize,
    pub clip: f64,
    pub noise_multiplier: f64,
    /// The global cumulative privacy budget after this round.
    pub epsilon: f64,
    pub delta: f64,
    pub prior_model_hash: String,
    pub candidate_model_hash: String,
    pub prior_benchmark_mse: f64,
    pub candidate_benchmark_mse: f64,
    pub promoted: bool,
    pub round_seed: u64,
    /// SHA-256 over the canonical JSON of every field above (this field excluded).
    pub manifest_hash: String,
}

impl RoundManifest {
    /// Recompute the content hash over all fields except `manifest_hash` and
    /// verify it matches — proves the manifest is untampered and reproducible.
    pub fn verify_hash(&self) -> bool {
        let mut bare = self.clone();
        bare.manifest_hash = String::new();
        content_hash(&bare) == self.manifest_hash
    }
}

fn content_hash(m: &RoundManifest) -> String {
    // serde_json with sorted-ish struct field order is stable for our struct.
    let json = serde_json::to_string(m).expect("manifest serialises");
    let mut h = Sha256::new();
    h.update(b"sovereigndna.federated.round.v1");
    h.update(json.as_bytes());
    let d = h.finalize();
    let mut s = String::with_capacity(64);
    for b in d {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// The outcome of a round: the manifest, the (possibly promoted) model, and the
/// per-node masked+noised shares that would cross the boundary in a real system.
#[derive(Debug, Clone)]
pub struct RoundOutcome {
    pub manifest: RoundManifest,
    pub model: Model,
    pub masked_shares: Vec<Vec<f64>>,
}

/// Execute one benchmark-gated federated round.
///
/// `prior` is the current shared model; `nodes` are the simulated participants;
/// `benchmark` is the fixed held-out synthetic evaluation set; `accountant`
/// tracks the running global privacy budget (mutated by one Gaussian round).
pub fn run_round(
    prior: &Model,
    nodes: &[LocalNode],
    benchmark: &[LocalExample],
    cfg: &RoundConfig,
    accountant: &mut PrivacyAccountant,
) -> RoundOutcome {
    let k = prior.len();
    let mech = cfg.mechanism();

    // 1–3: local gradients → clip + noise → secure aggregate.
    let noised: Vec<Vec<f64>> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let grad = node.local_gradient(prior);
            // Independent noise stream per node, deterministic from the round seed.
            let mut rng = SplitMix64::new(cfg.round_seed ^ (0x1000 + i as u64));
            mech.privatize(&grad, &mut rng)
        })
        .collect();

    let (masked_shares, agg) = secure_agg::secure_sum(&noised, cfg.round_seed);
    accountant.compose_gaussian(cfg.noise_multiplier);

    // 4: FedSGD step with the averaged aggregate gradient.
    let n = nodes.len().max(1) as f64;
    let mut candidate_weights = prior.weights.clone();
    for (w, g) in candidate_weights.iter_mut().zip(&agg) {
        *w -= cfg.learning_rate * (g / n);
    }
    let candidate = Model::from_weights(candidate_weights);

    // 5: benchmark gate.
    let prior_mse = prior.mse(benchmark);
    let cand_mse = candidate.mse(benchmark);
    let promoted = cand_mse < prior_mse;
    let model = if promoted { candidate.clone() } else { prior.clone() };

    // 6: manifest.
    let mut manifest = RoundManifest {
        prototype: true,
        round: accountant.rounds,
        num_nodes: nodes.len(),
        num_variants: k,
        clip: cfg.clip,
        noise_multiplier: cfg.noise_multiplier,
        epsilon: accountant.epsilon(cfg.delta),
        delta: cfg.delta,
        prior_model_hash: prior.content_hash(),
        candidate_model_hash: candidate.content_hash(),
        prior_benchmark_mse: prior_mse,
        candidate_benchmark_mse: cand_mse,
        promoted,
        round_seed: cfg.round_seed,
        manifest_hash: String::new(),
    };
    manifest.manifest_hash = content_hash(&manifest);

    RoundOutcome {
        manifest,
        model,
        masked_shares,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build a synthetic world where phenotype ≈ true_w · dosage, so training
    // toward `true_w` provably lowers benchmark MSE.
    fn make_world(true_w: &[f64], n_nodes: usize, per_node: usize) -> (Vec<LocalNode>, Vec<LocalExample>) {
        let k = true_w.len();
        let mut rng = SplitMix64::new(2026);
        let mut gen = |count: usize| -> Vec<LocalExample> {
            (0..count)
                .map(|_| {
                    let dosages: Vec<f64> = (0..k).map(|_| (rng.next_u64() % 3) as f64).collect();
                    let y: f64 = true_w.iter().zip(&dosages).map(|(w, x)| w * x).sum();
                    LocalExample { dosages, phenotype: y }
                })
                .collect()
        };
        let nodes = (0..n_nodes)
            .map(|i| LocalNode::new(format!("node-{i}"), gen(per_node)))
            .collect();
        let benchmark = gen(200);
        (nodes, benchmark)
    }

    fn cfg(seed: u64, z: f64) -> RoundConfig {
        RoundConfig {
            clip: 5.0,
            noise_multiplier: z,
            learning_rate: 0.02,
            delta: 1e-5,
            round_seed: seed,
        }
    }

    #[test]
    fn manifest_is_content_addressed_and_reproducible() {
        let (nodes, bench) = make_world(&[1.0, -0.5, 0.25], 3, 40);
        let prior = Model::zeros(3);

        let mut acc1 = PrivacyAccountant::new();
        let out1 = run_round(&prior, &nodes, &bench, &cfg(77, 0.4), &mut acc1);
        let mut acc2 = PrivacyAccountant::new();
        let out2 = run_round(&prior, &nodes, &bench, &cfg(77, 0.4), &mut acc2);

        // Same inputs + seed → identical manifest hash & model.
        assert_eq!(out1.manifest.manifest_hash, out2.manifest.manifest_hash);
        assert_eq!(out1.model.weights, out2.model.weights);
        assert!(out1.manifest.verify_hash());
        // A different seed → different hash (different noise/masks path).
        let mut acc3 = PrivacyAccountant::new();
        let out3 = run_round(&prior, &nodes, &bench, &cfg(78, 0.4), &mut acc3);
        assert_ne!(out1.manifest.manifest_hash, out3.manifest.manifest_hash);
    }

    #[test]
    fn good_round_is_promoted_bad_round_is_gated_out() {
        let (nodes, bench) = make_world(&[1.0, -0.5, 0.25], 4, 60);
        let prior = Model::zeros(3);

        // Low noise → the aggregate points toward the true weights → MSE drops.
        let mut acc = PrivacyAccountant::new();
        let good = run_round(&prior, &nodes, &bench, &cfg(1, 0.2), &mut acc);
        assert!(good.manifest.promoted, "low-noise informative round should promote");
        assert!(good.manifest.candidate_benchmark_mse < good.manifest.prior_benchmark_mse);
        assert_eq!(good.model.weights, {
            // promoted → returned model is the candidate (not prior zeros)
            assert_ne!(good.model.weights, prior.weights);
            good.model.weights.clone()
        });

        // A model already at the optimum: any noised step can only hurt → gated out.
        let optimum = Model::from_weights(vec![1.0, -0.5, 0.25]);
        let mut acc2 = PrivacyAccountant::new();
        let bad = run_round(&optimum, &nodes, &bench, &cfg(9, 3.0), &mut acc2);
        assert!(!bad.manifest.promoted, "a noisy step from optimum must be rejected");
        assert_eq!(bad.model.weights, optimum.weights, "prior model must stand");
    }

    #[test]
    fn multi_round_training_converges_and_spends_budget() {
        let (nodes, bench) = make_world(&[0.8, 0.4, -0.6, 0.2], 5, 50);
        let mut model = Model::zeros(4);
        let mut acc = PrivacyAccountant::new();
        let start_mse = model.mse(&bench);

        for r in 0..15 {
            let out = run_round(&model, &nodes, &bench, &cfg(100 + r, 0.3), &mut acc);
            model = out.model; // gate ensures this never increases MSE
        }
        let end_mse = model.mse(&bench);
        assert!(end_mse < start_mse, "training should improve: {start_mse} -> {end_mse}");
        // 15 composed rounds must have spent a positive, bounded budget.
        let eps = acc.epsilon(1e-5);
        assert!(eps > 0.0 && eps.is_finite());
        assert_eq!(acc.rounds, 15);
    }
}
