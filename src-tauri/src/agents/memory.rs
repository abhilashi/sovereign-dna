//! Agent persistent memory — the findings log (Phase 3.1 + 3.3).
//!
//! An agent's "memory" is the append-only log of what it has found across runs,
//! plus the timestamp of its last run (so scheduling and "new since last time"
//! digests work). This module defines the record types plus an
//! [`AgentMemory`] trait so the run loop is storage-agnostic and unit-testable;
//! the real app backs it with SQLite (see `crate::agents` db adapter), tests use
//! [`InMemoryMemory`].
//!
//! **Genome-data-safe:** a finding records public rsIDs it referenced and
//! human-readable text — never a raw genotype string keyed to identity in a way
//! that would be unsafe to share. (Findings are *not* part of a shared agent
//! definition; only definitions are shared — see Phase 3.9.)

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The kind of thing an agent found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    /// A result produced by running one of the agent's skills.
    SkillResult,
    /// A matched research article (PubMed-style watcher agents).
    ResearchArticle,
    /// A free-form note (e.g. an LLM summary of a run).
    Note,
}

/// One item an agent recorded in its memory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentFinding {
    /// Stable id, derived from the run id + ordinal (deterministic).
    pub id: String,
    /// The agent that produced it.
    pub agent_id: String,
    /// The run that produced it.
    pub run_id: String,
    pub kind: FindingKind,
    pub title: String,
    pub detail: String,
    /// Public rsIDs this finding references (safe to store/share).
    #[serde(default)]
    pub rsids: Vec<String>,
    /// RFC3339 creation timestamp.
    pub created_at: String,
    /// Whether the user has seen this finding (drives the "new" badge).
    #[serde(default)]
    pub seen: bool,
}

impl AgentFinding {
    /// Build a finding with a deterministic id derived from `(run_id, ordinal)`.
    pub fn new(
        agent_id: &str,
        run_id: &str,
        ordinal: usize,
        kind: FindingKind,
        title: impl Into<String>,
        detail: impl Into<String>,
        rsids: Vec<String>,
        created_at: &str,
    ) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(run_id.as_bytes());
        hasher.update(b"#");
        hasher.update(ordinal.to_le_bytes());
        let digest = hasher.finalize();
        let mut short = String::with_capacity(16);
        for b in &digest[..8] {
            short.push_str(&format!("{b:02x}"));
        }
        AgentFinding {
            id: format!("f-{short}"),
            agent_id: agent_id.to_string(),
            run_id: run_id.to_string(),
            kind,
            title: title.into(),
            detail: detail.into(),
            rsids,
            created_at: created_at.to_string(),
            seen: false,
        }
    }
}

/// Storage-agnostic persistent memory for agents.
///
/// The run loop appends findings and reads the last-run timestamp through this
/// trait, so it never touches the database directly.
pub trait AgentMemory {
    /// Append a finding to the log.
    fn append(&mut self, finding: AgentFinding);
    /// All findings for `agent_id`, newest first.
    fn findings(&self, agent_id: &str) -> Vec<AgentFinding>;
    /// RFC3339 timestamp of the most recent run recorded for `agent_id`.
    fn last_run_at(&self, agent_id: &str) -> Option<String>;
    /// Record that `agent_id` ran at `at` (RFC3339).
    fn record_run(&mut self, agent_id: &str, at: &str);
    /// Mark a finding seen; returns whether it existed.
    fn mark_seen(&mut self, finding_id: &str) -> bool;
    /// Count of unseen findings for `agent_id`.
    fn unseen_count(&self, agent_id: &str) -> usize {
        self.findings(agent_id).iter().filter(|f| !f.seen).count()
    }
}

/// A simple in-memory [`AgentMemory`] for tests and as a fallback.
#[derive(Debug, Default)]
pub struct InMemoryMemory {
    findings: Vec<AgentFinding>,
    last_run: std::collections::HashMap<String, String>,
}

impl InMemoryMemory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.findings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
    }
}

impl AgentMemory for InMemoryMemory {
    fn append(&mut self, finding: AgentFinding) {
        self.findings.push(finding);
    }

    fn findings(&self, agent_id: &str) -> Vec<AgentFinding> {
        let mut out: Vec<AgentFinding> = self
            .findings
            .iter()
            .filter(|f| f.agent_id == agent_id)
            .cloned()
            .collect();
        // Newest first (created_at is RFC3339 → lexicographically sortable).
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        out
    }

    fn last_run_at(&self, agent_id: &str) -> Option<String> {
        self.last_run.get(agent_id).cloned()
    }

    fn record_run(&mut self, agent_id: &str, at: &str) {
        self.last_run.insert(agent_id.to_string(), at.to_string());
    }

    fn mark_seen(&mut self, finding_id: &str) -> bool {
        for f in &mut self.findings {
            if f.id == finding_id {
                f.seen = true;
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(agent: &str, run: &str, ord: usize, at: &str) -> AgentFinding {
        AgentFinding::new(
            agent,
            run,
            ord,
            FindingKind::SkillResult,
            "title",
            "detail",
            vec!["rs1".into()],
            at,
        )
    }

    #[test]
    fn finding_id_is_deterministic_per_run_ordinal() {
        let a = finding("ag", "run1", 0, "2026-07-16T00:00:00Z");
        let b = finding("ag", "run1", 0, "2026-07-16T00:00:00Z");
        let c = finding("ag", "run1", 1, "2026-07-16T00:00:00Z");
        assert_eq!(a.id, b.id);
        assert_ne!(a.id, c.id);
        assert!(a.id.starts_with("f-"));
    }

    #[test]
    fn append_and_query_newest_first() {
        let mut mem = InMemoryMemory::new();
        mem.append(finding("ag", "run1", 0, "2026-07-16T00:00:00Z"));
        mem.append(finding("ag", "run2", 0, "2026-07-16T02:00:00Z"));
        mem.append(finding("other", "run3", 0, "2026-07-16T03:00:00Z"));
        let f = mem.findings("ag");
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].created_at, "2026-07-16T02:00:00Z"); // newest first
    }

    #[test]
    fn record_and_read_last_run() {
        let mut mem = InMemoryMemory::new();
        assert!(mem.last_run_at("ag").is_none());
        mem.record_run("ag", "2026-07-16T01:00:00Z");
        assert_eq!(mem.last_run_at("ag").as_deref(), Some("2026-07-16T01:00:00Z"));
    }

    #[test]
    fn mark_seen_and_unseen_count() {
        let mut mem = InMemoryMemory::new();
        let f = finding("ag", "run1", 0, "2026-07-16T00:00:00Z");
        let id = f.id.clone();
        mem.append(f);
        mem.append(finding("ag", "run1", 1, "2026-07-16T00:00:00Z"));
        assert_eq!(mem.unseen_count("ag"), 2);
        assert!(mem.mark_seen(&id));
        assert_eq!(mem.unseen_count("ag"), 1);
        assert!(!mem.mark_seen("f-nonexistent"));
    }
}
