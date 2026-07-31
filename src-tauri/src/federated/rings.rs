//! Rings-based [`Transport`] adapter over a **process-separation RPC boundary**.
//! **[PROTOTYPE/EXPERIMENTAL — activation gated behind the `rings-transport`
//! cargo feature, OFF by default]**
//!
//! # Why a process boundary (the license reason)
//! [RingsNetwork/rings](https://github.com/RingsNetwork/rings) is a browser-native
//! P2P overlay (Chord DHT + WebRTC datachannels, DID identity, namespace-scoped
//! protocol runtime) — exactly the substrate a real Phase-4 federated round would
//! want. **But `rings` is licensed GPL-3.0 (copyleft).** Statically linking it
//! into the SovereignDNA core would virally impose GPL on the whole application
//! and foreclose the license options Robin/Abhilash still owe a decision on.
//!
//! So this adapter treats Rings as a **separate process** reached over an
//! RPC/IPC boundary — precisely the deployment `rings` already supports: the
//! `rings` daemon exposes a **JSON-RPC-over-HTTP** interface (`crates/rpc`). Our
//! core depends only on the *JSON wire shapes* (method names + params), never on
//! any Rings Rust type. That keeps the core license-flexible: the GPL code runs
//! in its own process and we speak to it the way any client speaks to any server.
//!
//! # What this spike proves vs. what it stubs
//! * **Proves:** the [`Transport`] seam works over a JSON-RPC boundary; the FL
//!   round is registered as a **Rings namespace protocol** (`registerService`)
//!   and DP-noised aggregates are exchanged via the real Rings topic verbs
//!   (`publishMessageToTopic` / `fetchTopicMessages`); nothing but a serialized
//!   [`ShareMessage`] ever crosses; the boundary invariant is enforced.
//! * **Stubs:** [`StubRingsNode`] stands in for the real `rings` daemon. It
//!   implements the same JSON-RPC method names and message semantics **entirely
//!   over JSON strings** (so the core never touches a Rings Rust type), but it
//!   keeps topics in an in-memory map instead of routing over a live DHT/WebRTC
//!   overlay. No real DID handshake, no WebRTC, no network — those are the
//!   production follow-ups (see `mod.rs` and the spike RESULT.md).
//!
//! # Not linked, not vendored
//! This module has **no dependency on the `rings` crate**. It mirrors the public
//! JSON-RPC method surface (see `crates/rpc/src/method.rs` upstream) so a real
//! [`RingsRpc`] client (an HTTP client POSTing to a `rings` daemon, or an FFI
//! shim to a co-process) is a drop-in replacement for [`StubRingsRpc`].

use serde::{Deserialize, Serialize};

use super::transport::{guard_outbound, RoundTopic, ShareMessage, Transport, TransportError};

/// Whether the Rings transport is *activated*. `false` unless the crate is built
/// with the `rings-transport` feature. The pure adapter + stub always compile (so
/// CI type-checks them); this const gates whether a default build would ever
/// select the Rings path, and — in production — whether the optional real
/// `rings`-daemon RPC client is compiled in. Mirrors
/// [`super::PROTOTYPE_ENABLED`].
pub const RINGS_TRANSPORT_ENABLED: bool = cfg!(feature = "rings-transport");

// ---------------------------------------------------------------------------
// The JSON-RPC wire (the process-separation boundary).
//
// Everything below crosses the boundary as *JSON text only*. `RingsRpc::call`
// takes a JSON-RPC request string and returns a JSON-RPC response string; no
// Rust value is shared across the boundary. That is the whole point: swap
// `StubRingsRpc` for an HTTP client to a real (GPL) `rings` daemon and the core
// is unchanged and unlinked.
// ---------------------------------------------------------------------------

/// A minimal JSON-RPC 2.0 request, matching what the `rings` daemon accepts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    /// One of the Rings method names, e.g. `"registerService"`.
    pub method: String,
    pub params: serde_json::Value,
}

/// A minimal JSON-RPC 2.0 response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
}

/// The Rings JSON-RPC method names this adapter uses. These are the *real*
/// upstream method strings (`crates/rpc/src/method.rs`), reproduced here so the
/// stub and any production client speak the identical wire vocabulary.
pub mod method {
    /// Register a namespaced service/protocol on the node.
    pub const REGISTER_SERVICE: &str = "registerService";
    /// Discover a namespaced service by name.
    pub const LOOKUP_SERVICE: &str = "lookupService";
    /// Append a message to a topic (our DP-noised share carrier).
    pub const PUBLISH_MESSAGE_TO_TOPIC: &str = "publishMessageToTopic";
    /// Fetch the messages appended to a topic (the aggregator's collect).
    pub const FETCH_TOPIC_MESSAGES: &str = "fetchTopicMessages";
    /// Retrieve this node's DID (identity), for completeness / logging.
    pub const NODE_DID: &str = "nodeDid";
}

/// The process-separation seam. An implementor is *another process* (or a stub
/// standing in for one) reachable only via JSON-RPC text. The GPL `rings` code,
/// if any, lives entirely behind this trait.
pub trait RingsRpc {
    /// Send one JSON-RPC request (as text) and return the response text. In
    /// production this is an HTTP POST to a `rings` daemon; here it is the stub.
    fn call(&mut self, request_json: &str) -> Result<String, TransportError>;
}

// ---------------------------------------------------------------------------
// StubRingsNode — stands in for a real `rings` daemon process.
//
// It only ever sees JSON strings, exactly as a networked daemon would. It keeps
// registered namespaces and per-topic message logs in memory. This models the
// Rings namespace-protocol semantics (register a service; publish/fetch topic
// messages) without any DHT/WebRTC/network — the honest stub the spike calls for.
// ---------------------------------------------------------------------------

/// In-memory stand-in for a separate `rings` daemon. Speaks JSON-RPC text.
#[derive(Debug, Default)]
pub struct StubRingsNode {
    /// Simulated node DID (identity). A real node derives this from a keypair.
    did: String,
    /// Registered namespaces/services (`registerService`).
    services: std::collections::BTreeSet<String>,
    /// Topic → ordered list of raw JSON message payloads (`publishMessageToTopic`).
    topics: std::collections::BTreeMap<String, Vec<serde_json::Value>>,
}

impl StubRingsNode {
    pub fn new(did: impl Into<String>) -> Self {
        Self {
            did: did.into(),
            ..Default::default()
        }
    }

    /// Process one JSON-RPC request string, mutate state, return a response
    /// string. This is the "far side" of the process boundary.
    pub fn handle(&mut self, request_json: &str) -> String {
        let req: JsonRpcRequest = match serde_json::from_str(request_json) {
            Ok(r) => r,
            Err(e) => return Self::error(0, format!("parse error: {e}")),
        };
        let id = req.id;
        match req.method.as_str() {
            method::NODE_DID => Self::ok(id, serde_json::json!({ "did": self.did })),
            method::REGISTER_SERVICE => {
                let name = req.params.get("name").and_then(|v| v.as_str());
                match name {
                    Some(n) => {
                        self.services.insert(n.to_string());
                        self.topics.entry(n.to_string()).or_default();
                        Self::ok(id, serde_json::json!({ "registered": n }))
                    }
                    None => Self::error(id, "registerService: missing 'name'".into()),
                }
            }
            method::LOOKUP_SERVICE => {
                let name = req.params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                Self::ok(id, serde_json::json!({ "found": self.services.contains(name) }))
            }
            method::PUBLISH_MESSAGE_TO_TOPIC => {
                let topic = req.params.get("topic").and_then(|v| v.as_str());
                let data = req.params.get("data").cloned();
                match (topic, data) {
                    (Some(t), Some(d)) => {
                        if !self.services.contains(t) {
                            return Self::error(id, format!("topic '{t}' not registered"));
                        }
                        self.topics.entry(t.to_string()).or_default().push(d);
                        Self::ok(id, serde_json::json!({ "ok": true }))
                    }
                    _ => Self::error(id, "publishMessageToTopic: missing 'topic'/'data'".into()),
                }
            }
            method::FETCH_TOPIC_MESSAGES => {
                let topic = req.params.get("topic").and_then(|v| v.as_str());
                match topic {
                    Some(t) => {
                        let msgs = self.topics.get(t).cloned().unwrap_or_default();
                        Self::ok(id, serde_json::json!({ "messages": msgs }))
                    }
                    None => Self::error(id, "fetchTopicMessages: missing 'topic'".into()),
                }
            }
            other => Self::error(id, format!("unsupported method '{other}'")),
        }
    }

    fn ok(id: u64, result: serde_json::Value) -> String {
        serde_json::to_string(&JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        })
        .expect("response serialises")
    }

    fn error(id: u64, message: String) -> String {
        serde_json::to_string(&JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(serde_json::json!({ "code": -32000, "message": message })),
        })
        .expect("error serialises")
    }
}

/// A [`RingsRpc`] that drives an in-process [`StubRingsNode`] — used for the
/// spike and tests. It still only exchanges JSON *text* with the node, so it is
/// a faithful stand-in for an HTTP client talking to a separate `rings` process.
#[derive(Debug, Default)]
pub struct StubRingsRpc {
    node: StubRingsNode,
    next_id: u64,
}

impl StubRingsRpc {
    pub fn new(did: impl Into<String>) -> Self {
        Self {
            node: StubRingsNode::new(did),
            next_id: 0,
        }
    }
}

impl RingsRpc for StubRingsRpc {
    fn call(&mut self, request_json: &str) -> Result<String, TransportError> {
        // The request has already been serialised to text by the caller; the
        // node returns text. Nothing but strings crosses this boundary.
        Ok(self.node.handle(request_json))
    }
}

// ---------------------------------------------------------------------------
// RingsTransport — the core-side adapter implementing `Transport` over `RingsRpc`.
// ---------------------------------------------------------------------------

/// A [`Transport`] that registers the FL round as a Rings namespace protocol and
/// exchanges DP-noised shares over the Rings JSON-RPC topic verbs — across a
/// process boundary. Generic over the [`RingsRpc`] client so the stub (spike) and
/// a real HTTP-to-`rings`-daemon client are interchangeable.
pub struct RingsTransport<R: RingsRpc> {
    rpc: R,
    next_id: u64,
}

impl RingsTransport<StubRingsRpc> {
    /// Construct the spike transport backed by an in-process stub Rings node.
    pub fn stub(did: impl Into<String>) -> Self {
        Self::with_rpc(StubRingsRpc::new(did))
    }
}

impl<R: RingsRpc> RingsTransport<R> {
    /// Construct over any [`RingsRpc`] client. In production this would be an
    /// HTTP client pointed at a running `rings` daemon (the GPL code, separate
    /// process); here it is [`StubRingsRpc`].
    pub fn with_rpc(rpc: R) -> Self {
        Self { rpc, next_id: 0 }
    }

    fn rpc_call(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, TransportError> {
        self.next_id += 1;
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: self.next_id,
            method: method.to_string(),
            params,
        };
        // Serialise to text — this is what crosses the process boundary.
        let request_json =
            serde_json::to_string(&req).map_err(|e| TransportError::Rpc(e.to_string()))?;
        let response_json = self.rpc.call(&request_json)?;
        let resp: JsonRpcResponse =
            serde_json::from_str(&response_json).map_err(|e| TransportError::Rpc(e.to_string()))?;
        if let Some(err) = resp.error {
            return Err(TransportError::Rpc(err.to_string()));
        }
        resp.result
            .ok_or_else(|| TransportError::Rpc("empty JSON-RPC result".into()))
    }
}

impl<R: RingsRpc> Transport for RingsTransport<R> {
    fn register_round(&mut self, topic: &RoundTopic) -> Result<(), TransportError> {
        // Register the FL round as a Rings namespace-scoped service/protocol.
        self.rpc_call(
            method::REGISTER_SERVICE,
            serde_json::json!({
                "name": topic.topic(),
                "namespace": topic.namespace,
                "round": topic.round,
            }),
        )?;
        Ok(())
    }

    fn publish_share(
        &mut self,
        topic: &RoundTopic,
        msg: &ShareMessage,
    ) -> Result<(), TransportError> {
        // Enforce the device boundary *before* anything leaves this process.
        guard_outbound(msg)?;
        // Serialise the DP-noised masked share as the topic message payload.
        let data =
            serde_json::to_value(msg).map_err(|e| TransportError::Boundary(e.to_string()))?;
        self.rpc_call(
            method::PUBLISH_MESSAGE_TO_TOPIC,
            serde_json::json!({ "topic": topic.topic(), "data": data }),
        )?;
        Ok(())
    }

    fn collect_shares(
        &mut self,
        topic: &RoundTopic,
        expected: usize,
    ) -> Result<Vec<ShareMessage>, TransportError> {
        let result = self.rpc_call(
            method::FETCH_TOPIC_MESSAGES,
            serde_json::json!({ "topic": topic.topic() }),
        )?;
        let msgs = result
            .get("messages")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut shares: Vec<ShareMessage> = Vec::with_capacity(msgs.len());
        for m in msgs {
            let share: ShareMessage =
                serde_json::from_value(m).map_err(|e| TransportError::Rpc(e.to_string()))?;
            // Defence in depth: re-check every payload as it re-enters the core.
            guard_outbound(&share)?;
            shares.push(share);
        }
        if shares.len() != expected {
            return Err(TransportError::Incomplete {
                expected,
                got: shares.len(),
            });
        }
        shares.sort_by_key(|m| m.node_index);
        Ok(shares)
    }

    fn kind(&self) -> &'static str {
        "rings-rpc-stub"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noised_share(round: u64, node: usize, values: Vec<f64>) -> ShareMessage {
        ShareMessage {
            round,
            node_index: node,
            values,
            dp_noised: true,
            masked: true,
        }
    }

    #[cfg(feature = "rings-transport")]
    #[test]
    fn feature_flag_reflects_activation() {
        assert!(RINGS_TRANSPORT_ENABLED);
    }

    #[cfg(not(feature = "rings-transport"))]
    #[test]
    fn rings_transport_is_off_by_default() {
        assert!(!RINGS_TRANSPORT_ENABLED);
    }

    #[test]
    fn only_json_text_crosses_the_boundary() {
        // The stub node only ever receives &str and returns String — proving no
        // Rust type is shared across the process-separation seam.
        let mut node = StubRingsNode::new("did:rings:stub");
        let resp = node.handle(r#"{"jsonrpc":"2.0","id":1,"method":"nodeDid","params":{}}"#);
        assert!(resp.contains("did:rings:stub"));
        let parsed: JsonRpcResponse = serde_json::from_str(&resp).unwrap();
        assert_eq!(parsed.id, 1);
    }

    #[test]
    fn round_registers_as_namespace_and_exchanges_one_aggregate() {
        let topic = RoundTopic::new(3, 12345);
        let mut tp = RingsTransport::stub("did:rings:node-a");
        tp.register_round(&topic).unwrap();

        // Two simulated nodes each publish ONE DP-noised masked share.
        tp.publish_share(&topic, &noised_share(3, 0, vec![0.11, -0.22]))
            .unwrap();
        tp.publish_share(&topic, &noised_share(3, 1, vec![-0.05, 0.31]))
            .unwrap();

        let got = tp.collect_shares(&topic, 2).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].node_index, 0);
        assert_eq!(got[1].node_index, 1);
        assert!(got.iter().all(|m| m.dp_noised && m.masked));
        assert_eq!(tp.kind(), "rings-rpc-stub");
    }

    #[test]
    fn publish_to_unregistered_topic_is_rejected_by_node() {
        let topic = RoundTopic::new(1, 1);
        let mut tp = RingsTransport::stub("did:rings:node-b");
        // No register_round → the stub node refuses the publish.
        let err = tp
            .publish_share(&topic, &noised_share(1, 0, vec![0.1]))
            .unwrap_err();
        assert!(matches!(err, TransportError::Rpc(_)));
    }

    #[test]
    fn a_clear_gradient_is_refused_at_the_boundary() {
        // An un-noised share (dp_noised = false) is refused before it can cross.
        let topic = RoundTopic::new(1, 1);
        let mut tp = RingsTransport::stub("did:rings:node-c");
        tp.register_round(&topic).unwrap();
        let mut clear = noised_share(1, 0, vec![10.0, -20.0]);
        clear.dp_noised = false;
        let err = tp.publish_share(&topic, &clear).unwrap_err();
        assert!(matches!(err, TransportError::Boundary(_)));
    }
}
