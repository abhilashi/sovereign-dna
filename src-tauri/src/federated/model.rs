//! Bounded on-device model + local trainer (Phase 4.1). **[PROTOTYPE/EXPERIMENTAL]**
//!
//! The federated task is deliberately **tiny and bounded**: refine the weights
//! of a small linear **polygenic-score (PRS) re-weighting** model over a fixed
//! panel of `K` variants. A model is just a weight vector `w ∈ ℝ^K`; a
//! prediction is `ŷ = w · x` where `x ∈ {0,1,2}^K` is the additive genotype
//! dosage of an individual. The loss is mean-squared error against a phenotype
//! label `y`.
//!
//! This is intentionally a *toy* model — the point of the prototype is to get
//! the **privacy mechanism** (clipping + DP noise + secure aggregation + budget
//! accounting) demonstrably correct, not to ship a clinically valid PRS. We use
//! **FedSGD**: each node computes one local gradient over its private examples;
//! that gradient is the only thing that is ever a *candidate* to leave the
//! device, and only after clipping + noise + masking (see `dp` / `secure_agg`).
//!
//! **Privacy invariant:** an individual's `dosages`/`phenotype` never leave
//! `LocalNode`. The only value that crosses the (simulated) device boundary is
//! a clipped-and-noised gradient, additively masked so no single party sees it.

use serde::{Deserialize, Serialize};

/// A shared linear model over a fixed variant panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model {
    /// One weight per variant in the panel (length `K`).
    pub weights: Vec<f64>,
}

impl Model {
    /// A zero-initialised model over `k` variants.
    pub fn zeros(k: usize) -> Self {
        Self {
            weights: vec![0.0; k],
        }
    }

    pub fn from_weights(weights: Vec<f64>) -> Self {
        Self { weights }
    }

    pub fn len(&self) -> usize {
        self.weights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.weights.is_empty()
    }

    /// Predict the phenotype for one dosage vector: `ŷ = w · x`.
    pub fn predict(&self, dosages: &[f64]) -> f64 {
        debug_assert_eq!(dosages.len(), self.weights.len());
        self.weights
            .iter()
            .zip(dosages)
            .map(|(w, x)| w * x)
            .sum()
    }

    /// Mean-squared error over a labelled dataset.
    pub fn mse(&self, data: &[LocalExample]) -> f64 {
        if data.is_empty() {
            return 0.0;
        }
        let mut acc = 0.0;
        for ex in data {
            let err = self.predict(&ex.dosages) - ex.phenotype;
            acc += err * err;
        }
        acc / data.len() as f64
    }

    /// A stable content hash of the model weights (bit-exact, order-preserving),
    /// used to content-address rounds. Reproducible across runs/platforms.
    pub fn content_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(b"sovereigndna.federated.model.v1");
        h.update((self.weights.len() as u64).to_le_bytes());
        for w in &self.weights {
            // Canonicalise -0.0 to 0.0 so equal models hash equally.
            let bits = if *w == 0.0 { 0.0f64 } else { *w }.to_bits();
            h.update(bits.to_le_bytes());
        }
        let d = h.finalize();
        let mut s = String::with_capacity(64);
        for b in d {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }
}

/// One private training example held on a device — **never** leaves it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalExample {
    /// Additive genotype dosages over the panel (0/1/2). Synthetic in tests.
    pub dosages: Vec<f64>,
    /// The (private) phenotype label.
    pub phenotype: f64,
}

/// One simulated participating device holding a private local dataset.
#[derive(Debug, Clone)]
pub struct LocalNode {
    pub id: String,
    examples: Vec<LocalExample>,
}

impl LocalNode {
    pub fn new(id: impl Into<String>, examples: Vec<LocalExample>) -> Self {
        Self {
            id: id.into(),
            examples,
        }
    }

    pub fn num_examples(&self) -> usize {
        self.examples.len()
    }

    /// Compute the **local MSE gradient** of the shared `model` over this node's
    /// private data. For `ŷ = w·x`, `∂/∂w_k MSE = mean_i 2·(ŷ_i − y_i)·x_ik`.
    ///
    /// This gradient is the *only* candidate that can leave the device, and only
    /// after `dp::clip_l2` + `dp::add_gaussian_noise` + `secure_agg` masking.
    pub fn local_gradient(&self, model: &Model) -> Vec<f64> {
        let k = model.len();
        let mut grad = vec![0.0; k];
        if self.examples.is_empty() {
            return grad;
        }
        for ex in &self.examples {
            debug_assert_eq!(ex.dosages.len(), k);
            let err = model.predict(&ex.dosages) - ex.phenotype;
            for (g, x) in grad.iter_mut().zip(&ex.dosages) {
                *g += 2.0 * err * x;
            }
        }
        let n = self.examples.len() as f64;
        for g in &mut grad {
            *g /= n;
        }
        grad
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(dosages: &[f64], y: f64) -> LocalExample {
        LocalExample {
            dosages: dosages.to_vec(),
            phenotype: y,
        }
    }

    #[test]
    fn predict_is_dot_product() {
        let m = Model::from_weights(vec![0.5, -1.0, 2.0]);
        assert_eq!(m.predict(&[2.0, 1.0, 0.0]), 1.0 - 1.0 + 0.0);
    }

    #[test]
    fn zero_model_has_zero_gradient_when_labels_zero() {
        let node = LocalNode::new("n", vec![ex(&[1.0, 0.0], 0.0), ex(&[0.0, 2.0], 0.0)]);
        let g = node.local_gradient(&Model::zeros(2));
        assert_eq!(g, vec![0.0, 0.0]);
    }

    #[test]
    fn gradient_step_reduces_loss() {
        // A node that can perfectly fit y = 1.0 * x0.
        let node = LocalNode::new(
            "n",
            vec![ex(&[1.0, 0.0], 1.0), ex(&[2.0, 0.0], 2.0), ex(&[0.0, 1.0], 0.0)],
        );
        let mut model = Model::zeros(2);
        let before = model.mse(node_examples(&node));
        // One gradient-descent step.
        let g = node.local_gradient(&model);
        for (w, gk) in model.weights.iter_mut().zip(&g) {
            *w -= 0.1 * gk;
        }
        let after = model.mse(node_examples(&node));
        assert!(after < before, "loss should drop: {before} -> {after}");
    }

    // Test-only accessor.
    fn node_examples(n: &LocalNode) -> &[LocalExample] {
        &n.examples
    }

    #[test]
    fn content_hash_is_stable_and_sensitive() {
        let a = Model::from_weights(vec![1.0, 2.0, 3.0]);
        let b = Model::from_weights(vec![1.0, 2.0, 3.0]);
        let c = Model::from_weights(vec![1.0, 2.0, 3.0001]);
        assert_eq!(a.content_hash(), b.content_hash());
        assert_ne!(a.content_hash(), c.content_hash());
        // -0.0 and 0.0 hash equally.
        assert_eq!(
            Model::from_weights(vec![0.0]).content_hash(),
            Model::from_weights(vec![-0.0]).content_hash()
        );
    }
}
