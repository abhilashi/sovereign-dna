//! Per-action privacy & consent ledger (Phase 3.7).
//!
//! This is a **first-class, legally load-bearing primitive**. Every action an
//! agent takes is authorised against explicit, **revocable** consent and then
//! recorded, so the user can see *exactly* what each agent did — in particular
//! **what left the device, which rsIDs, and to which endpoint**. It turns the
//! app's §1.4 "the privacy guarantee rests on code review" gap into a visible,
//! auditable, user-facing feature.
//!
//! Design invariants:
//! * **Genome-data-safe by construction.** An [`Egress`] can carry only *public
//!   identifiers* (dbSNP rsIDs / public coordinates) — there is no field for a
//!   genotype, and [`Egress::assert_public_only`] rejects anything that is not a
//!   public identifier. A genotype string like `"AG"` cannot pass.
//! * **Consent gates egress.** A remote egress action is denied unless an active
//!   (non-revoked) [`ConsentGrant`] covers it. Local actions (on-device reads,
//!   localhost LLM) never egress and need no consent, but are still recorded.
//! * **Append-only audit.** Revoking consent stops *future* actions; it never
//!   erases the historical record.
//!
//! Pure logic only — persistence lives in [`crate::agents::store`], the
//! user-facing surface in [`crate::commands::agents`].

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The class of action an agent performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// Read genotypes from the local database (no egress).
    ReadVariants,
    /// Run a skill locally (no egress).
    RunSkill,
    /// Query a reference DB endpoint (rsIDs may leave the device).
    QueryReference,
    /// Query PubMed / NCBI E-utilities (rsIDs leave the device).
    QueryPubMed,
    /// Summarise with the local Ollama model (localhost — no remote egress).
    LlmLocal,
    /// Summarise with a remote model, e.g. Claude (curated context leaves).
    LlmRemote,
    /// Raise a desktop notification (local).
    Notify,
    /// Contribute a **differentially-private, secure-aggregated** model update to
    /// the (prototype) federated network — Phase 4.8. Only DP-noised aggregate
    /// metadata and public rsIDs may accompany it; never a genotype or a
    /// per-individual clear gradient (enforced by [`Egress::assert_public_only`]
    /// and `crate::federated::boundary`).
    FederatedUpdate,
}

impl ActionKind {
    /// Whether this kind *inherently* sends bytes off the device to a remote host.
    pub fn is_remote(&self) -> bool {
        matches!(self, ActionKind::LlmRemote | ActionKind::FederatedUpdate)
            || matches!(self, ActionKind::QueryReference | ActionKind::QueryPubMed)
    }
}

/// A description of bytes leaving the current process.
///
/// **Genome-data-safe:** only public identifiers may appear here. There is no
/// field capable of holding a genotype; `assert_public_only` enforces that the
/// listed identifiers really are public rsIDs / coordinates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Egress {
    /// Destination host (e.g. `"eutils.ncbi.nlm.nih.gov"`, `"api.anthropic.com"`,
    /// `"localhost:11434"`).
    pub endpoint: String,
    /// True when the destination is on-device (localhost/loopback).
    pub is_local: bool,
    /// The public identifiers that left (dbSNP rsIDs / `chr:pos` coordinates).
    #[serde(default)]
    pub identifiers: Vec<String>,
    /// Human-readable description of the payload (never a genotype).
    pub description: String,
}

impl Egress {
    /// Reject any identifier that is not a public dbSNP rsID or a `chr:pos`
    /// coordinate — this is the structural guard that a genotype (e.g. `"AG"`)
    /// can never be recorded as "left the device".
    pub fn assert_public_only(&self) -> Result<(), String> {
        for id in &self.identifiers {
            if !is_public_identifier(id) {
                return Err(format!(
                    "egress identifier '{id}' is not a public rsID/coordinate — refusing to record it as egress"
                ));
            }
        }
        Ok(())
    }
}

/// A public variant identifier: `rs<digits>` or `<chr>:<pos>` (optionally with
/// a build prefix). Deliberately does **not** match genotype strings.
pub fn is_public_identifier(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    // rsID form: rs followed by digits.
    let lower = s.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("rs") {
        return !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit());
    }
    // chr:pos form: at least one ':' with a numeric position on the right.
    if let Some((chrom, pos)) = s.rsplit_once(':') {
        let chrom_ok = !chrom.is_empty()
            && chrom
                .trim_start_matches("chr")
                .chars()
                .all(|c| c.is_ascii_alphanumeric());
        let pos_ok = !pos.is_empty() && pos.bytes().all(|b| b.is_ascii_digit());
        return chrom_ok && pos_ok;
    }
    false
}

/// What a consent grant permits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsentScope {
    /// Action kinds this grant authorises.
    pub actions: Vec<ActionKind>,
    /// Whether actions that send data to a *remote* host are permitted.
    pub allow_remote_egress: bool,
    /// Optional cap on which rsIDs may be read/sent. Empty = the agent's own
    /// declared scope governs (no extra restriction here).
    #[serde(default)]
    pub rsid_allowlist: Vec<String>,
}

/// An explicit, revocable permission the user gives an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsentGrant {
    pub id: String,
    pub agent_id: String,
    pub scope: ConsentScope,
    pub granted_at: String,
    /// RFC3339 revocation time, if revoked. Historical ledger entries keep their
    /// original authorisation; only *future* actions are affected.
    #[serde(default)]
    pub revoked_at: Option<String>,
    #[serde(default)]
    pub note: String,
}

impl ConsentGrant {
    pub fn is_active(&self) -> bool {
        self.revoked_at.is_none()
    }

    /// Whether this grant authorises `action` (kind allowed, remote egress
    /// permitted if needed, and rsIDs within the allowlist when one is set).
    pub fn permits(&self, action: &ProposedAction) -> bool {
        if self.agent_id != action.agent_id || !self.is_active() {
            return false;
        }
        if !self.scope.actions.contains(&action.kind) {
            return false;
        }
        let remote = action
            .egress
            .as_ref()
            .map(|e| !e.is_local)
            .unwrap_or(false);
        if remote && !self.scope.allow_remote_egress {
            return false;
        }
        if !self.scope.rsid_allowlist.is_empty() {
            let allowed = |r: &String| {
                self.scope
                    .rsid_allowlist
                    .iter()
                    .any(|a| a.eq_ignore_ascii_case(r))
            };
            if !action.rsids.iter().all(allowed) {
                return false;
            }
        }
        true
    }
}

/// An action an agent proposes to take, submitted to the ledger *before* it is
/// performed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedAction {
    pub agent_id: String,
    pub run_id: String,
    pub kind: ActionKind,
    /// Public rsIDs involved in the action.
    #[serde(default)]
    pub rsids: Vec<String>,
    /// Present iff the action sends bytes over a socket (local or remote).
    #[serde(default)]
    pub egress: Option<Egress>,
    pub description: String,
}

/// The authorisation outcome for a proposed action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ActionOutcome {
    /// Purely local action (no remote egress) — allowed, no consent required.
    AllowedLocal,
    /// Remote egress authorised by an active consent grant.
    AllowedByConsent { consent_id: String },
    /// Blocked: no active consent, or an unsafe (non-public) payload.
    Denied { reason: String },
}

impl ActionOutcome {
    pub fn is_allowed(&self) -> bool {
        !matches!(self, ActionOutcome::Denied { .. })
    }
}

/// A recorded, immutable ledger entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerEntry {
    pub id: String,
    pub agent_id: String,
    pub run_id: String,
    pub timestamp: String,
    pub kind: ActionKind,
    #[serde(default)]
    pub rsids: Vec<String>,
    #[serde(default)]
    pub egress: Option<Egress>,
    pub outcome: ActionOutcome,
    pub description: String,
}

/// Authorise a proposed action against the current consent grants.
///
/// Order of checks:
/// 1. any egress payload must contain only public identifiers (genome-safety);
/// 2. an action with no egress, or only *local* egress, is `AllowedLocal`;
/// 3. a *remote* egress action needs an active consent grant that covers it,
///    else it is `Denied`.
pub fn authorize(action: &ProposedAction, consents: &[ConsentGrant]) -> ActionOutcome {
    if let Some(eg) = &action.egress {
        if let Err(reason) = eg.assert_public_only() {
            return ActionOutcome::Denied { reason };
        }
        if !eg.is_local {
            // Remote egress: require an active covering consent.
            return match consents.iter().find(|c| c.permits(action)) {
                Some(c) => ActionOutcome::AllowedByConsent {
                    consent_id: c.id.clone(),
                },
                None => ActionOutcome::Denied {
                    reason: format!(
                        "no active consent authorises {:?} egress to {}",
                        action.kind, eg.endpoint
                    ),
                },
            };
        }
    }
    ActionOutcome::AllowedLocal
}

/// Authorise and turn a proposed action into an immutable [`LedgerEntry`].
///
/// The caller records the returned entry and — crucially — must only *perform*
/// the action if `entry.outcome.is_allowed()`.
pub fn record(action: ProposedAction, consents: &[ConsentGrant], timestamp: &str) -> LedgerEntry {
    let outcome = authorize(&action, consents);
    let mut hasher = Sha256::new();
    hasher.update(action.run_id.as_bytes());
    hasher.update(b"|");
    hasher.update(action.description.as_bytes());
    hasher.update(b"|");
    hasher.update(timestamp.as_bytes());
    let digest = hasher.finalize();
    let mut short = String::with_capacity(16);
    for b in &digest[..8] {
        short.push_str(&format!("{b:02x}"));
    }
    LedgerEntry {
        id: format!("l-{short}"),
        agent_id: action.agent_id,
        run_id: action.run_id,
        timestamp: timestamp.to_string(),
        kind: action.kind,
        rsids: action.rsids,
        egress: action.egress,
        outcome,
        description: action.description,
    }
}

/// A running summary of what an agent has sent off the device — the user-facing
/// "privacy ledger" view.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EgressSummary {
    /// Total recorded actions.
    pub total_actions: usize,
    /// Actions that stayed fully on-device.
    pub local_actions: usize,
    /// Actions that sent data to a remote host.
    pub remote_actions: usize,
    /// Actions that were blocked.
    pub denied_actions: usize,
    /// Distinct rsIDs that left the device (to any remote host).
    pub rsids_sent: Vec<String>,
    /// Remote endpoints contacted.
    pub endpoints: Vec<String>,
}

/// Summarise a slice of ledger entries into an [`EgressSummary`].
pub fn summarize_egress(entries: &[LedgerEntry]) -> EgressSummary {
    let mut s = EgressSummary::default();
    let mut rsids = std::collections::BTreeSet::new();
    let mut endpoints = std::collections::BTreeSet::new();
    for e in entries {
        s.total_actions += 1;
        match &e.outcome {
            ActionOutcome::Denied { .. } => s.denied_actions += 1,
            _ => {}
        }
        let remote = e.egress.as_ref().map(|g| !g.is_local).unwrap_or(false)
            && e.outcome.is_allowed();
        if remote {
            s.remote_actions += 1;
            if let Some(g) = &e.egress {
                endpoints.insert(g.endpoint.clone());
                for id in &g.identifiers {
                    rsids.insert(id.clone());
                }
            }
        } else if e.outcome.is_allowed() {
            s.local_actions += 1;
        }
    }
    s.rsids_sent = rsids.into_iter().collect();
    s.endpoints = endpoints.into_iter().collect();
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(id: &str, agent: &str, actions: Vec<ActionKind>, remote: bool) -> ConsentGrant {
        ConsentGrant {
            id: id.into(),
            agent_id: agent.into(),
            scope: ConsentScope {
                actions,
                allow_remote_egress: remote,
                rsid_allowlist: vec![],
            },
            granted_at: "2026-07-16T00:00:00Z".into(),
            revoked_at: None,
            note: String::new(),
        }
    }

    fn pubmed_action(agent: &str, rsids: Vec<&str>) -> ProposedAction {
        ProposedAction {
            agent_id: agent.into(),
            run_id: "run-1".into(),
            kind: ActionKind::QueryPubMed,
            rsids: rsids.iter().map(|s| s.to_string()).collect(),
            egress: Some(Egress {
                endpoint: "eutils.ncbi.nlm.nih.gov".into(),
                is_local: false,
                identifiers: rsids.iter().map(|s| s.to_string()).collect(),
                description: "PubMed ESearch by rsID".into(),
            }),
            description: "watch PubMed for APOE".into(),
        }
    }

    #[test]
    fn public_identifier_recognises_rsids_and_coords_but_not_genotypes() {
        assert!(is_public_identifier("rs429358"));
        assert!(is_public_identifier("RS7412"));
        assert!(is_public_identifier("chr19:44908684"));
        assert!(is_public_identifier("19:44908684"));
        // genotypes and junk are rejected
        assert!(!is_public_identifier("AG"));
        assert!(!is_public_identifier("AA"));
        assert!(!is_public_identifier("rs"));
        assert!(!is_public_identifier(""));
        assert!(!is_public_identifier("APOE")); // gene symbol is not an identifier here
    }

    #[test]
    fn egress_rejects_non_public_identifier() {
        let eg = Egress {
            endpoint: "api.anthropic.com".into(),
            is_local: false,
            identifiers: vec!["rs1".into(), "AG".into()], // AG is a genotype!
            description: "context".into(),
        };
        assert!(eg.assert_public_only().is_err());
    }

    #[test]
    fn local_action_is_allowed_without_consent() {
        let action = ProposedAction {
            agent_id: "ag".into(),
            run_id: "r".into(),
            kind: ActionKind::ReadVariants,
            rsids: vec!["rs1".into()],
            egress: None,
            description: "read variants".into(),
        };
        assert_eq!(authorize(&action, &[]), ActionOutcome::AllowedLocal);
    }

    #[test]
    fn localhost_llm_is_local_and_needs_no_consent() {
        let action = ProposedAction {
            agent_id: "ag".into(),
            run_id: "r".into(),
            kind: ActionKind::LlmLocal,
            rsids: vec![],
            egress: Some(Egress {
                endpoint: "localhost:11434".into(),
                is_local: true,
                identifiers: vec![],
                description: "ollama summary".into(),
            }),
            description: "summarise".into(),
        };
        assert_eq!(authorize(&action, &[]), ActionOutcome::AllowedLocal);
    }

    #[test]
    fn remote_egress_denied_without_consent() {
        let action = pubmed_action("ag", vec!["rs429358"]);
        let out = authorize(&action, &[]);
        assert!(matches!(out, ActionOutcome::Denied { .. }));
        assert!(!out.is_allowed());
    }

    #[test]
    fn remote_egress_allowed_with_active_consent() {
        let action = pubmed_action("ag", vec!["rs429358"]);
        let consents = vec![grant("c1", "ag", vec![ActionKind::QueryPubMed], true)];
        assert_eq!(
            authorize(&action, &consents),
            ActionOutcome::AllowedByConsent {
                consent_id: "c1".into()
            }
        );
    }

    #[test]
    fn revoked_consent_no_longer_authorises() {
        let action = pubmed_action("ag", vec!["rs429358"]);
        let mut c = grant("c1", "ag", vec![ActionKind::QueryPubMed], true);
        c.revoked_at = Some("2026-07-16T01:00:00Z".into());
        assert!(matches!(
            authorize(&action, &[c]),
            ActionOutcome::Denied { .. }
        ));
    }

    #[test]
    fn consent_for_other_agent_or_wrong_kind_does_not_apply() {
        let action = pubmed_action("ag", vec!["rs1"]);
        // wrong agent
        let c1 = grant("c1", "other", vec![ActionKind::QueryPubMed], true);
        // wrong kind
        let c2 = grant("c2", "ag", vec![ActionKind::LlmRemote], true);
        // remote not allowed
        let c3 = grant("c3", "ag", vec![ActionKind::QueryPubMed], false);
        assert!(matches!(
            authorize(&action, &[c1, c2, c3]),
            ActionOutcome::Denied { .. }
        ));
    }

    #[test]
    fn rsid_allowlist_bounds_what_can_be_sent() {
        let mut c = grant("c1", "ag", vec![ActionKind::QueryPubMed], true);
        c.scope.rsid_allowlist = vec!["rs429358".into()];
        // in-allowlist rsID → allowed
        assert!(authorize(&pubmed_action("ag", vec!["rs429358"]), &[c.clone()]).is_allowed());
        // out-of-allowlist rsID → denied
        assert!(!authorize(&pubmed_action("ag", vec!["rs7412"]), &[c]).is_allowed());
    }

    #[test]
    fn record_produces_stable_id_and_correct_outcome() {
        let action = pubmed_action("ag", vec!["rs429358"]);
        let consents = vec![grant("c1", "ag", vec![ActionKind::QueryPubMed], true)];
        let e1 = record(action.clone(), &consents, "2026-07-16T00:00:00Z");
        let e2 = record(action, &consents, "2026-07-16T00:00:00Z");
        assert_eq!(e1.id, e2.id);
        assert!(e1.id.starts_with("l-"));
        assert!(e1.outcome.is_allowed());
    }

    #[test]
    fn egress_summary_counts_and_dedupes() {
        let consents = vec![grant("c1", "ag", vec![ActionKind::QueryPubMed], true)];
        let mut entries = Vec::new();
        entries.push(record(pubmed_action("ag", vec!["rs1", "rs2"]), &consents, "t1"));
        entries.push(record(pubmed_action("ag", vec!["rs2", "rs3"]), &consents, "t2"));
        // a denied remote action (no consent for LlmRemote)
        let mut denied = pubmed_action("ag", vec!["rs9"]);
        denied.kind = ActionKind::LlmRemote;
        denied.egress.as_mut().unwrap().endpoint = "api.anthropic.com".into();
        entries.push(record(denied, &consents, "t3"));
        // a local action
        entries.push(record(
            ProposedAction {
                agent_id: "ag".into(),
                run_id: "run-1".into(),
                kind: ActionKind::ReadVariants,
                rsids: vec!["rs1".into()],
                egress: None,
                description: "read".into(),
            },
            &consents,
            "t4",
        ));

        let s = summarize_egress(&entries);
        assert_eq!(s.total_actions, 4);
        assert_eq!(s.remote_actions, 2); // two allowed PubMed egresses
        assert_eq!(s.local_actions, 1);
        assert_eq!(s.denied_actions, 1);
        assert_eq!(s.rsids_sent, vec!["rs1", "rs2", "rs3"]); // deduped + sorted
        assert_eq!(s.endpoints, vec!["eutils.ncbi.nlm.nih.gov"]);
    }
}
