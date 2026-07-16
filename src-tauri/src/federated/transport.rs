//! Transport seam for exchanging DP-noised aggregate payloads between Phase-4
//! federated nodes. **[PROTOTYPE/EXPERIMENTAL]**
//!
//! PR #113 exchanged per-node updates *in process* (`secure_agg::secure_sum`
//! called directly inside `round::run_round`). This module introduces a
//! [`Transport`] seam so the same round can exchange its **DP-noised, secure-
//! masked** shares over *any* substrate:
//!
//! * [`InProcessTransport`] — the default; reproduces PR #113's in-process
//!   simulation bit-for-bit (nothing about the round's numeric result changes).
//! * `super::rings::RingsTransport` — routes the identical [`ShareMessage`]s over
//!   a **process-separation RPC boundary** to a (stub or real) Rings node, so the
//!   GPL-3.0 Rings code never links into this crate. See that module.
//!
//! ## The boundary invariant
//! The *only* payload type that crosses a [`Transport`] during a round is a
//! [`ShareMessage`]: a per-node **masked + DP-noised** share. By construction it
//! has no field capable of holding a genotype, a phenotype, or a clear
//! (un-noised, un-masked) gradient. Every implementation additionally routes each
//! outbound payload through [`guard_outbound`], which re-checks it against the
//! Phase-4.8 device-boundary rules ([`super::boundary`]) at runtime.

use serde::{Deserialize, Serialize};

/// Identifies one federated-learning round as a routable topic / protocol
/// namespace. Mirrors a Rings *namespace-scoped protocol* address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundTopic {
    /// Stable protocol namespace (a Rings node routes inbound messages by this).
    pub namespace: String,
    /// The round id within that namespace.
    pub round: u64,
    /// The public round seed so pairwise masks line up across nodes. This is a
    /// routing/coordination value, **not** a secret and **not** a private datum.
    pub round_seed: u64,
}

impl RoundTopic {
    /// The versioned FL-round protocol namespace.
    pub const NAMESPACE: &'static str = "sovereigndna.fl.round.v1";

    pub fn new(round: u64, round_seed: u64) -> Self {
        Self {
            namespace: Self::NAMESPACE.to_string(),
            round,
            round_seed,
        }
    }

    /// Fully-qualified topic string used for routing (`namespace/round`).
    pub fn topic(&self) -> String {
        format!("{}/{}", self.namespace, self.round)
    }
}

/// The single payload type that crosses a [`Transport`] during a round: a
/// per-node **masked (secure-agg) + DP-noised** update share.
///
/// There is deliberately **no** field able to carry a genotype dosage, a
/// phenotype label, or an un-noised per-individual gradient. The `dp_noised` /
/// `masked` flags are provenance markers asserting the values already passed
/// clip + Gaussian noise ([`super::dp`]) and pairwise masking
/// ([`super::secure_agg`]); [`guard_outbound`] refuses to send anything that has
/// not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareMessage {
    pub round: u64,
    pub node_index: usize,
    /// Masked + DP-noised values. Any *single* one is one-time-padded and
    /// therefore indistinguishable from random to an observer.
    pub values: Vec<f64>,
    /// True iff the values passed the DP (clip + Gaussian noise) mechanism.
    pub dp_noised: bool,
    /// True iff the values were pairwise-masked for secure aggregation.
    pub masked: bool,
}

/// Errors a transport can raise. Kept small and `String`-backed so the seam does
/// not force a heavy error dependency onto callers.
#[derive(Debug, Clone, PartialEq)]
pub enum TransportError {
    /// A round topic was used before being registered.
    NotRegistered(String),
    /// The underlying RPC / IPC boundary failed (stub or real Rings node).
    Rpc(String),
    /// A payload was refused by the device-boundary guard.
    Boundary(String),
    /// Fewer (or more) shares were available than the round expected.
    Incomplete { expected: usize, got: usize },
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::NotRegistered(t) => write!(f, "round topic '{t}' is not registered"),
            TransportError::Rpc(e) => write!(f, "transport RPC error: {e}"),
            TransportError::Boundary(e) => write!(f, "device-boundary refusal: {e}"),
            TransportError::Incomplete { expected, got } => {
                write!(f, "incomplete round: expected {expected} shares, got {got}")
            }
        }
    }
}

impl std::error::Error for TransportError {}

/// Runtime device-boundary guard applied by **every** transport before a payload
/// is allowed to leave the node. Re-uses the Phase-4.8 boundary rules so the
/// transport seam cannot become a hole around them: a share must be marked
/// DP-noised + masked, be non-empty, and contain only finite values (an
/// [`super::boundary::OutboundArtifact::MaskedNoisedShare`]).
pub fn guard_outbound(msg: &ShareMessage) -> Result<(), TransportError> {
    if !(msg.dp_noised && msg.masked) {
        return Err(TransportError::Boundary(
            "share is not marked DP-noised + masked — refusing to transmit".into(),
        ));
    }
    let artifact = super::boundary::OutboundArtifact::MaskedNoisedShare {
        round: msg.round,
        values: msg.values.clone(),
    };
    super::boundary::assert_leaves_boundary(&artifact).map_err(TransportError::Boundary)
}

/// Abstracts how one federated round exchanges DP-noised aggregate payloads
/// between nodes.
///
/// The contract is intentionally tiny and mirrors a publish/collect topic on a
/// namespace-scoped overlay: register the round, every node publishes its one
/// DP-noised masked share, and the aggregator collects them. Implementations
/// **must** call [`guard_outbound`] before a payload leaves the node.
pub trait Transport {
    /// Register the FL round as a routable protocol / namespace on the transport.
    fn register_round(&mut self, topic: &RoundTopic) -> Result<(), TransportError>;

    /// Publish this node's DP-noised masked share into the round.
    fn publish_share(
        &mut self,
        topic: &RoundTopic,
        msg: &ShareMessage,
    ) -> Result<(), TransportError>;

    /// Collect exactly `expected` published shares (the aggregator's view),
    /// ordered by `node_index` so aggregation is deterministic.
    fn collect_shares(
        &mut self,
        topic: &RoundTopic,
        expected: usize,
    ) -> Result<Vec<ShareMessage>, TransportError>;

    /// A short human label for logging / manifests (e.g. `"in-process"`).
    fn kind(&self) -> &'static str;
}

/// Default transport: the PR #113 in-process simulation, now expressed through
/// the [`Transport`] seam. Bit-for-bit equivalent to the previous direct
/// `secure_agg::secure_sum` path — collecting the shares in `node_index` order
/// yields the same aggregate and therefore the same round manifest.
#[derive(Debug, Default)]
pub struct InProcessTransport {
    rounds: std::collections::BTreeMap<String, Vec<ShareMessage>>,
}

impl InProcessTransport {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Transport for InProcessTransport {
    fn register_round(&mut self, topic: &RoundTopic) -> Result<(), TransportError> {
        self.rounds.entry(topic.topic()).or_default();
        Ok(())
    }

    fn publish_share(
        &mut self,
        topic: &RoundTopic,
        msg: &ShareMessage,
    ) -> Result<(), TransportError> {
        guard_outbound(msg)?;
        let bucket = self
            .rounds
            .get_mut(&topic.topic())
            .ok_or_else(|| TransportError::NotRegistered(topic.topic()))?;
        bucket.push(msg.clone());
        Ok(())
    }

    fn collect_shares(
        &mut self,
        topic: &RoundTopic,
        expected: usize,
    ) -> Result<Vec<ShareMessage>, TransportError> {
        let bucket = self
            .rounds
            .get(&topic.topic())
            .ok_or_else(|| TransportError::NotRegistered(topic.topic()))?;
        if bucket.len() != expected {
            return Err(TransportError::Incomplete {
                expected,
                got: bucket.len(),
            });
        }
        let mut shares = bucket.clone();
        shares.sort_by_key(|m| m.node_index);
        Ok(shares)
    }

    fn kind(&self) -> &'static str {
        "in-process"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn share(node: usize, values: Vec<f64>) -> ShareMessage {
        ShareMessage {
            round: 1,
            node_index: node,
            values,
            dp_noised: true,
            masked: true,
        }
    }

    #[test]
    fn topic_is_namespace_scoped() {
        let t = RoundTopic::new(7, 42);
        assert_eq!(t.namespace, RoundTopic::NAMESPACE);
        assert_eq!(t.topic(), "sovereigndna.fl.round.v1/7");
    }

    #[test]
    fn in_process_round_trips_shares_in_node_order() {
        let mut tp = InProcessTransport::new();
        let topic = RoundTopic::new(1, 99);
        tp.register_round(&topic).unwrap();
        // Publish out of order; collect must be sorted by node_index.
        tp.publish_share(&topic, &share(2, vec![3.0, 3.0])).unwrap();
        tp.publish_share(&topic, &share(0, vec![1.0, 1.0])).unwrap();
        tp.publish_share(&topic, &share(1, vec![2.0, 2.0])).unwrap();
        let got = tp.collect_shares(&topic, 3).unwrap();
        assert_eq!(got.iter().map(|m| m.node_index).collect::<Vec<_>>(), [0, 1, 2]);
    }

    #[test]
    fn publish_before_register_is_rejected() {
        let mut tp = InProcessTransport::new();
        let topic = RoundTopic::new(1, 1);
        let err = tp.publish_share(&topic, &share(0, vec![1.0])).unwrap_err();
        assert!(matches!(err, TransportError::NotRegistered(_)));
    }

    #[test]
    fn guard_refuses_unmarked_or_nonfinite_share() {
        // Not marked DP-noised → refused before leaving the node.
        let mut bad = share(0, vec![1.0, 2.0]);
        bad.dp_noised = false;
        assert!(matches!(guard_outbound(&bad), Err(TransportError::Boundary(_))));
        // Non-finite value → refused by the boundary rules.
        let nan = share(0, vec![f64::NAN]);
        assert!(matches!(guard_outbound(&nan), Err(TransportError::Boundary(_))));
        // A proper masked+noised share passes.
        assert!(guard_outbound(&share(0, vec![0.1, -0.2])).is_ok());
    }

    #[test]
    fn incomplete_round_is_detected() {
        let mut tp = InProcessTransport::new();
        let topic = RoundTopic::new(1, 1);
        tp.register_round(&topic).unwrap();
        tp.publish_share(&topic, &share(0, vec![1.0])).unwrap();
        let err = tp.collect_shares(&topic, 3).unwrap_err();
        assert!(matches!(err, TransportError::Incomplete { expected: 3, got: 1 }));
    }
}
