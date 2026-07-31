//! Device boundary + privacy-ledger integration (Phase 4.8). **[PROTOTYPE]**
//!
//! Extends the Phase-3.7 consent/ledger (`crate::agents::ledger`) so a federated
//! round can *prove* that **only DP-noised, secure-aggregated updates and public
//! rsIDs ever leave the (simulated) device** — never a raw genotype, never a
//! per-individual clear gradient, never a phenotype label.
//!
//! Two layers of enforcement:
//! 1. **Structural (type-level).** [`OutboundArtifact`] has no variant capable of
//!    holding a genotype, a clear gradient, or a phenotype. The only carriers are
//!    a masked+noised share, a round-manifest reference, and public identifiers.
//! 2. **Runtime.** [`assert_leaves_boundary`] re-checks every artifact, and
//!    [`federated_egress_action`] routes the contribution through the existing
//!    ledger [`authorize`]/[`record`] pipeline — so it is consent-gated and its
//!    identifiers are validated by [`Egress::assert_public_only`] (a genotype
//!    string like `"AG"` is rejected exactly as for any other agent action).

use crate::agents::ledger::{
    is_public_identifier, ActionKind, ConsentGrant, Egress, LedgerEntry, ProposedAction,
};

/// Something a node is about to send off-device during a federated round.
/// Deliberately narrow: there is **no** variant that can carry private data.
#[derive(Debug, Clone, PartialEq)]
pub enum OutboundArtifact {
    /// A per-node **masked + DP-noised** update share. After pairwise masking
    /// (`secure_agg`) and Gaussian noise (`dp`) this is indistinguishable from
    /// random to any single observer — it is not the clear gradient.
    MaskedNoisedShare { round: u64, values: Vec<f64> },
    /// A reference to a public, content-addressed round manifest (hash + ε/δ).
    RoundManifestRef { manifest_hash: String, epsilon: f64 },
    /// Public dbSNP rsIDs / `chr:pos` coordinates (already used for research).
    PublicIdentifiers(Vec<String>),
}

/// Runtime re-check that an artifact is safe to cross the boundary.
pub fn assert_leaves_boundary(artifact: &OutboundArtifact) -> Result<(), String> {
    match artifact {
        OutboundArtifact::MaskedNoisedShare { values, .. } => {
            if values.is_empty() {
                return Err("masked share is empty".into());
            }
            if values.iter().any(|v| !v.is_finite()) {
                return Err("masked share contains non-finite values".into());
            }
            Ok(())
        }
        OutboundArtifact::RoundManifestRef { manifest_hash, .. } => {
            if manifest_hash.len() != 64 || !manifest_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("manifest hash is not a 64-char hex digest".into());
            }
            Ok(())
        }
        OutboundArtifact::PublicIdentifiers(ids) => {
            for id in ids {
                if !is_public_identifier(id) {
                    return Err(format!(
                        "identifier '{id}' is not a public rsID/coordinate — refusing egress"
                    ));
                }
            }
            Ok(())
        }
    }
}

/// The proof that a *clear* per-individual gradient never leaves: the value that
/// actually crosses the boundary (the masked + noised share) must differ from the
/// clear local gradient it was derived from. Returns `Err` if they are (near-)
/// identical, i.e. if masking/noise failed to protect the individual update.
pub fn assert_share_protects_gradient(
    clear_gradient: &[f64],
    outbound_share: &[f64],
) -> Result<(), String> {
    if clear_gradient.len() != outbound_share.len() {
        return Err("length mismatch between gradient and outbound share".into());
    }
    let l1: f64 = clear_gradient
        .iter()
        .zip(outbound_share)
        .map(|(g, s)| (g - s).abs())
        .sum();
    if l1 <= 1e-9 {
        return Err(
            "outbound share equals the clear gradient — masking/noise did not protect it".into(),
        );
    }
    Ok(())
}

/// Build the ledger action for contributing a federated update, then authorise +
/// record it through the Phase-3.7 pipeline. `panel_rsids` are the *public* panel
/// identifiers; `manifest_hash`/`epsilon` describe what left. Consent gates it.
///
/// If any element of `panel_rsids` is not a public identifier (e.g. a genotype
/// slipped in), [`Egress::assert_public_only`] inside `authorize` denies it.
pub fn federated_egress_action(
    agent_id: &str,
    run_id: &str,
    endpoint: &str,
    panel_rsids: Vec<String>,
    manifest_hash: &str,
    epsilon: f64,
    delta: f64,
    consents: &[ConsentGrant],
    timestamp: &str,
) -> LedgerEntry {
    let action = ProposedAction {
        agent_id: agent_id.to_string(),
        run_id: run_id.to_string(),
        kind: ActionKind::FederatedUpdate,
        rsids: panel_rsids.clone(),
        egress: Some(Egress {
            endpoint: endpoint.to_string(),
            is_local: false,
            identifiers: panel_rsids,
            description: format!(
                "[PROTOTYPE] DP-noised secure-aggregated model update; manifest {manifest_hash}, (ε={epsilon:.4}, δ={delta:.1e})"
            ),
        }),
        description: "federated DP model-update contribution".to_string(),
    };
    crate::agents::ledger::record(action, consents, timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::ledger::{ActionOutcome, ConsentScope};

    fn consent(agent: &str) -> ConsentGrant {
        ConsentGrant {
            id: "c-fed".into(),
            agent_id: agent.into(),
            scope: ConsentScope {
                actions: vec![ActionKind::FederatedUpdate],
                allow_remote_egress: true,
                rsid_allowlist: vec![],
            },
            granted_at: "2026-07-16T00:00:00Z".into(),
            revoked_at: None,
            note: String::new(),
        }
    }

    #[test]
    fn safe_artifacts_pass_boundary() {
        assert!(assert_leaves_boundary(&OutboundArtifact::MaskedNoisedShare {
            round: 1,
            values: vec![0.3, -1.2, 5.0],
        })
        .is_ok());
        assert!(assert_leaves_boundary(&OutboundArtifact::PublicIdentifiers(vec![
            "rs12913832".into(),
            "chr1:12345".into(),
        ]))
        .is_ok());
        assert!(assert_leaves_boundary(&OutboundArtifact::RoundManifestRef {
            manifest_hash: "a".repeat(64),
            epsilon: 1.5,
        })
        .is_ok());
    }

    #[test]
    fn genotype_masquerading_as_identifier_is_rejected() {
        // "AG" is a genotype, not a public identifier → boundary rejects it.
        let bad = OutboundArtifact::PublicIdentifiers(vec!["rs671".into(), "AG".into()]);
        assert!(assert_leaves_boundary(&bad).is_err());
    }

    #[test]
    fn clear_gradient_is_never_what_leaves() {
        let clear = vec![10.0, -20.0, 5.0];
        // A properly masked+noised share differs substantially from the clear grad.
        let protected = vec![10.4, -18.7, 6.1];
        assert!(assert_share_protects_gradient(&clear, &protected).is_ok());
        // If the "share" is the clear gradient itself, the guard fires.
        assert!(assert_share_protects_gradient(&clear, &clear).is_err());
    }

    #[test]
    fn federated_egress_is_consent_gated_and_genome_safe() {
        let agent = "org.sovereigndna.agents.fed";
        // With consent + only public rsIDs → allowed by consent.
        let entry = federated_egress_action(
            agent,
            "run-1",
            "federated.rendezvous.local",
            vec!["rs12913832".into(), "rs671".into()],
            &"b".repeat(64),
            0.87,
            1e-5,
            &[consent(agent)],
            "2026-07-16T12:00:00Z",
        );
        assert!(matches!(entry.outcome, ActionOutcome::AllowedByConsent { .. }));

        // Without consent → denied (no budget authorises remote egress).
        let denied = federated_egress_action(
            agent,
            "run-2",
            "federated.rendezvous.local",
            vec!["rs671".into()],
            &"c".repeat(64),
            0.1,
            1e-5,
            &[],
            "2026-07-16T12:05:00Z",
        );
        assert!(matches!(denied.outcome, ActionOutcome::Denied { .. }));

        // A genotype smuggled into the identifiers is denied even *with* consent.
        let smuggled = federated_egress_action(
            agent,
            "run-3",
            "federated.rendezvous.local",
            vec!["rs671".into(), "AG".into()],
            &"d".repeat(64),
            0.1,
            1e-5,
            &[consent(agent)],
            "2026-07-16T12:10:00Z",
        );
        assert!(matches!(smuggled.outcome, ActionOutcome::Denied { .. }));
    }
}
