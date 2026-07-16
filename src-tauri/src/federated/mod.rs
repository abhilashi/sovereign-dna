//! Phase 4 — privacy-preserving, self-improving P2P collective intelligence.
//! **[PROTOTYPE/EXPERIMENTAL — OFF BY DEFAULT]**
//!
//! ⚠️ **This is a bounded research prototype, not a production feature.** It
//! demonstrates the *core privacy mechanism* of Phase 4 honestly and with tests;
//! it does **not** ship a live network. A real launch is **legally gated** and
//! must not happen without a published threat model + ε budget, a DPIA, legal
//! review per jurisdiction, and an independent third-party privacy audit (see
//! the 4-phase spec §5.4 / §6.2 / §5.3-4.10).
//!
//! ## What this prototype demonstrates
//! * **4.1 Federated learning core** ([`model`]) — a tiny bounded PRS-reweighting
//!   linear model; each simulated node trains locally and produces only a
//!   gradient delta. Raw genotypes/phenotypes never leave [`model::LocalNode`].
//! * **4.2 Differential privacy** ([`dp`]) — per-update L2 **clipping** +
//!   calibrated **Gaussian noise**, with a **tracked global (ε, δ) budget** via a
//!   Rényi-DP accountant. This is the load-bearing part and is unit-tested for
//!   correctness (monotonicity + textbook ballpark).
//! * **4.3 Secure aggregation** ([`secure_agg`]) — Bonawitz-style pairwise
//!   additive masking so no single party sees an individual clear update, while
//!   the aggregate sum is exact.
//! * **4.7 Benchmark-gated rounds** ([`round`]) — a round is promoted only if it
//!   beats the prior model on a held-out synthetic benchmark; every round yields
//!   a reproducible, content-addressed [`round::RoundManifest`].
//! * **4.8 Privacy ledger** ([`boundary`]) — extends the Phase-3.7 consent/ledger
//!   to prove only DP-noised aggregates + public rsIDs cross the (simulated)
//!   device boundary; genotypes / per-individual clear gradients are refused.
//!
//! * **Transport seam** ([`transport`]) — how a round exchanges its DP-noised
//!   masked shares is abstracted behind a [`transport::Transport`] trait. The
//!   default [`transport::InProcessTransport`] keeps the in-process simulation;
//!   [`rings::RingsTransport`] (activation gated behind the `rings-transport`
//!   feature) routes the identical shares over a **process-separation JSON-RPC
//!   boundary** to a (stub or real) [Rings](https://github.com/RingsNetwork/rings)
//!   node — registering the round as a Rings namespace protocol — so the GPL-3.0
//!   Rings code never links into this crate.
//!
//! ## Explicitly out of scope for the prototype (gated / future — see PR)
//! * **4.4** real libp2p/WebRTC transport / content-addressed snapshot gossip.
//!   The [`transport`] seam + [`rings`] adapter prove the *boundary and API
//!   shape* against a stub Rings node; a live DID/WebRTC `rings` daemon, real
//!   peer discovery, and dropout recovery remain future work;
//! * **4.5** ZK/TEE attestation of correct DP application;
//! * **4.6 / 4.9** Sybil / poisoning defense + `$BEAST` staking incentives
//!   (only a benchmark gate + reproducible manifest are here);
//! * **4.10** independent privacy audit (a launch gate).
//!
//! ## Activation
//! The prototype is inert by default: nothing in the app invokes it and
//! [`PROTOTYPE_ENABLED`] is `false` unless the crate is built with the
//! `federated-prototype` feature. The pure modules still compile (so CI type-
//! checks them and the local verification harness runs their tests), but there is
//! no Tauri command surface wired in by default.
//!
//! The prototype is intentionally not yet called from the app, so its public API
//! is "unused" from the crate's perspective; silence dead-code noise crate-wide
//! for this inert module until a feature-gated command wires it in.
#![allow(dead_code)]

pub mod boundary;
pub mod dp;
pub mod model;
pub mod prng;
pub mod rings;
pub mod round;
pub mod secure_agg;
pub mod transport;

use dp::PrivacyAccountant;
use model::{LocalExample, LocalNode, Model};
use round::{RoundConfig, RoundManifest};

/// Whether the experimental federated prototype is compiled-in as active. Kept
/// `false` unless the `federated-prototype` cargo feature is enabled, so the
/// network layer can never run in a default build.
pub const PROTOTYPE_ENABLED: bool = cfg!(feature = "federated-prototype");

/// A one-line human banner making the prototype/gated status impossible to miss.
pub const PROTOTYPE_BANNER: &str =
    "[PROTOTYPE/EXPERIMENTAL] Phase-4 federated learning — OFF by default; \
     real launch is legally gated (DPIA + legal review + independent privacy audit).";

/// A full simulated federated experiment over several benchmark-gated rounds,
/// tracking the global privacy budget throughout. Returns every round's manifest
/// plus the final (promoted-or-held) model. Deterministic given the inputs.
///
/// This is the single orchestration entry a (future, feature-gated) command would
/// call. It performs **no networking** — all nodes are in-process simulations.
pub fn run_experiment(
    initial: Model,
    nodes: &[LocalNode],
    benchmark: &[LocalExample],
    base_config: RoundConfig,
    num_rounds: u64,
) -> ExperimentResult {
    let mut model = initial;
    let mut accountant = PrivacyAccountant::new();
    let mut manifests = Vec::with_capacity(num_rounds as usize);
    let start_mse = model.mse(benchmark);

    for r in 0..num_rounds {
        let cfg = RoundConfig {
            round_seed: base_config.round_seed.wrapping_add(r),
            ..base_config
        };
        let outcome = round::run_round(&model, nodes, benchmark, &cfg, &mut accountant);
        model = outcome.model;
        manifests.push(outcome.manifest);
    }

    let final_mse = model.mse(benchmark);
    ExperimentResult {
        final_model: model,
        manifests,
        epsilon: accountant.epsilon(base_config.delta),
        delta: base_config.delta,
        rounds: accountant.rounds,
        start_benchmark_mse: start_mse,
        final_benchmark_mse: final_mse,
    }
}

/// Summary of a simulated federated experiment.
#[derive(Debug, Clone)]
pub struct ExperimentResult {
    pub final_model: Model,
    pub manifests: Vec<RoundManifest>,
    /// Global cumulative privacy budget spent across all rounds.
    pub epsilon: f64,
    pub delta: f64,
    pub rounds: u64,
    pub start_benchmark_mse: f64,
    pub final_benchmark_mse: f64,
}

impl ExperimentResult {
    /// Number of rounds that beat the benchmark gate and were promoted.
    pub fn promoted_rounds(&self) -> usize {
        self.manifests.iter().filter(|m| m.promoted).count()
    }

    /// A printable report line — the "learnings not DNA" receipt.
    pub fn report(&self) -> String {
        format!(
            "{PROTOTYPE_BANNER}\n{} round(s), {} promoted; benchmark MSE {:.5} -> {:.5}; \
             global privacy budget (ε = {:.4}, δ = {:.1e})",
            self.rounds,
            self.promoted_rounds(),
            self.start_benchmark_mse,
            self.final_benchmark_mse,
            self.epsilon,
            self.delta,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prng::SplitMix64;

    fn synth_world(true_w: &[f64], nodes: usize, per_node: usize) -> (Vec<LocalNode>, Vec<LocalExample>) {
        let k = true_w.len();
        let mut rng = SplitMix64::new(4242);
        let mut gen = |count: usize| -> Vec<LocalExample> {
            (0..count)
                .map(|_| {
                    let dosages: Vec<f64> = (0..k).map(|_| (rng.next_u64() % 3) as f64).collect();
                    let y = true_w.iter().zip(&dosages).map(|(w, x)| w * x).sum();
                    LocalExample { dosages, phenotype: y }
                })
                .collect()
        };
        let ns = (0..nodes).map(|i| LocalNode::new(format!("n{i}"), gen(per_node))).collect();
        (ns, gen(300))
    }

    #[cfg(not(feature = "federated-prototype"))]
    #[test]
    fn prototype_is_off_by_default() {
        // The default build must never have the network layer compiled-in active.
        assert!(!PROTOTYPE_ENABLED);
    }

    #[cfg(feature = "federated-prototype")]
    #[test]
    fn prototype_flag_reflects_feature() {
        assert!(PROTOTYPE_ENABLED);
    }

    #[test]
    fn full_experiment_improves_model_within_a_budget() {
        let (nodes, bench) = synth_world(&[0.9, -0.4, 0.3, 0.6], 6, 60);
        let cfg = RoundConfig {
            clip: 5.0,
            noise_multiplier: 0.3,
            learning_rate: 0.02,
            delta: 1e-5,
            round_seed: 20260716,
        };
        let res = run_experiment(Model::zeros(4), &nodes, &bench, cfg, 20);

        // The gate guarantees monotone non-worsening; over 20 rounds it improves.
        assert!(res.final_benchmark_mse < res.start_benchmark_mse);
        assert!(res.promoted_rounds() > 0);
        assert_eq!(res.rounds, 20);
        assert_eq!(res.manifests.len(), 20);

        // A finite, positive, bounded global privacy budget was spent & reported.
        assert!(res.epsilon > 0.0 && res.epsilon.is_finite());
        assert!(res.report().contains("ε ="));

        // Every manifest is self-consistent (content hash verifies) and unique.
        for m in &res.manifests {
            assert!(m.verify_hash());
            assert!(m.prototype);
        }
        let hashes: std::collections::BTreeSet<_> =
            res.manifests.iter().map(|m| m.manifest_hash.clone()).collect();
        assert_eq!(hashes.len(), res.manifests.len(), "round manifests are distinct");
    }

    #[test]
    fn experiment_is_reproducible() {
        let (nodes, bench) = synth_world(&[1.0, 0.5], 4, 40);
        let cfg = RoundConfig {
            clip: 4.0,
            noise_multiplier: 0.5,
            learning_rate: 0.03,
            delta: 1e-6,
            round_seed: 1,
        };
        let a = run_experiment(Model::zeros(2), &nodes, &bench, cfg, 8);
        let b = run_experiment(Model::zeros(2), &nodes, &bench, cfg, 8);
        assert_eq!(a.final_model.weights, b.final_model.weights);
        assert_eq!(
            a.manifests.last().unwrap().manifest_hash,
            b.manifests.last().unwrap().manifest_hash
        );
        assert!((a.epsilon - b.epsilon).abs() < 1e-12);
    }
}
