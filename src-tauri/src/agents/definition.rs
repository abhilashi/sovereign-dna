//! Agent definition schema (Phase 3.1 + 3.3).
//!
//! A *user-created agent* is a saved, reusable analysis worker composed from:
//! * a **skill set** — ids of skills (Phase 2 registry) the agent runs;
//! * a **data scope** — which variants / research topics it is allowed to look at;
//! * an **LLM** — local Ollama by default, Claude opt-in (reuses the existing
//!   `local_llm` / `chat_with_claude` machinery in the command layer);
//! * persistent **memory** — its findings log (see [`crate::agents::memory`]).
//!
//! An [`AgentDefinition`] is a pure description. It contains **no genome data by
//! construction** — only public rsIDs, skill ids and topic keywords — so it is
//! safe to persist, and (Phase 3.9) to export/share as a signed manifest without
//! ever moving genotypes off the device.
//!
//! This module defines only the *schema* + validation. Execution lives in
//! [`crate::agents::runtime`].

use serde::{Deserialize, Serialize};

/// The agent-manifest schema version this build understands.
pub const AGENT_SCHEMA_VERSION: u32 = 1;

/// Which LLM an agent uses for its optional summarisation/reasoning step.
///
/// The variant only records *intent + model*; honoring it (and the egress rules)
/// is the command-layer summariser's job. The default — [`LlmConfig::LocalOllama`]
/// — keeps everything on device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum LlmConfig {
    /// Local Ollama (default). Fully private: nothing leaves the device.
    LocalOllama {
        #[serde(default)]
        model: Option<String>,
    },
    /// Opt-in Claude (user-supplied key). Only the curated context string built
    /// by the pipeline leaves the device — never the raw genome.
    Claude {
        #[serde(default)]
        model: Option<String>,
    },
    /// No LLM: a purely deterministic skill/report agent.
    None,
}

impl Default for LlmConfig {
    fn default() -> Self {
        LlmConfig::LocalOllama { model: None }
    }
}

impl LlmConfig {
    /// Whether this configuration can cause any bytes to leave the device.
    ///
    /// `LocalOllama` and `None` are fully local; only `Claude` egresses (the
    /// curated context string). Used by the safety layer (Phase 3.6).
    pub fn is_remote(&self) -> bool {
        matches!(self, LlmConfig::Claude { .. })
    }
}

/// The variants / topics an agent is allowed to read.
///
/// **Genome-data-safe:** every field is a public identifier (dbSNP rsIDs, skill
/// ids, research keywords) — never a genotype. The union of `rsids` and the
/// variants declared by the listed skills defines what the agent may look at.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataScope {
    /// Explicit dbSNP rsIDs the agent may read.
    #[serde(default)]
    pub rsids: Vec<String>,
    /// Research topics / keywords (for PubMed-style watcher agents).
    #[serde(default)]
    pub topics: Vec<String>,
}

impl DataScope {
    /// True when the scope pins no explicit rsIDs (the agent then reads whatever
    /// its declared skills declare).
    pub fn is_rsid_unrestricted(&self) -> bool {
        self.rsids.is_empty()
    }

    /// Whether `rsid` is within an explicit rsID scope. When no rsIDs are pinned,
    /// every rsID the declared skills touch is considered in scope.
    pub fn allows_rsid(&self, rsid: &str) -> bool {
        self.is_rsid_unrestricted() || self.rsids.iter().any(|r| r.eq_ignore_ascii_case(rsid))
    }
}

/// What causes an agent to run.
///
/// Time-based triggers are computed by the scheduler (Phase 3.2); event triggers
/// name a fleet event the scheduler watches. `Manual` runs only on user request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    /// Runs only when the user asks.
    Manual,
    /// Runs every `every_hours` hours (cron-like cadence).
    Interval { every_hours: u32 },
    /// Runs when a reference database (`clinvar`, `gwas_catalog`, ...) updates.
    OnReferenceUpdate { source: String },
    /// Runs when a new PubMed article matches the agent's scope.
    OnNewMatchedArticle,
}

impl Default for Trigger {
    fn default() -> Self {
        Trigger::Manual
    }
}

/// A user-created agent definition.
///
/// Contains **no genome data** — safe to persist and (Phase 3.9) share.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefinition {
    /// Manifest schema version. Must be `<= AGENT_SCHEMA_VERSION`.
    pub schema_version: u32,
    /// Stable, unique agent id (reverse-DNS style),
    /// e.g. `"org.sovereigndna.agents.variant-watcher"`.
    pub id: String,
    /// Semantic version of the agent *content*.
    pub version: String,
    /// Human-readable name.
    pub name: String,
    /// One-line description.
    pub description: String,
    /// Skills (built-in ids or registry content-ids) the agent runs each round.
    #[serde(default)]
    pub skill_ids: Vec<String>,
    /// What the agent may read.
    #[serde(default)]
    pub data_scope: DataScope,
    /// The LLM used for the optional summarisation step.
    #[serde(default)]
    pub llm: LlmConfig,
    /// What causes the agent to run.
    #[serde(default)]
    pub trigger: Trigger,
    /// Origin template id, when the agent was created from a template (Phase 3.4).
    #[serde(default)]
    pub template_id: Option<String>,
    /// Free-text instructions guiding the summarisation step.
    #[serde(default)]
    pub instructions: String,
    /// Mandatory user-facing disclaimer, surfaced with the agent's findings.
    pub disclaimer: String,
}

/// A structured validation error for an agent definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentError(pub String);

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid agent definition: {}", self.0)
    }
}

impl std::error::Error for AgentError {}

impl AgentDefinition {
    /// Parse an agent definition from JSON, validating it in the process.
    pub fn from_json(s: &str) -> Result<Self, AgentError> {
        let def: AgentDefinition =
            serde_json::from_str(s).map_err(|e| AgentError(format!("json: {e}")))?;
        def.validate()?;
        Ok(def)
    }

    /// Structurally validate the definition. Returns the first problem found.
    pub fn validate(&self) -> Result<(), AgentError> {
        if self.schema_version == 0 || self.schema_version > AGENT_SCHEMA_VERSION {
            return Err(AgentError(format!(
                "unsupported schemaVersion {} (this build supports 1..={})",
                self.schema_version, AGENT_SCHEMA_VERSION
            )));
        }
        for (field, val) in [
            ("id", &self.id),
            ("version", &self.version),
            ("name", &self.name),
            ("disclaimer", &self.disclaimer),
        ] {
            if val.trim().is_empty() {
                return Err(AgentError(format!("{field} must not be empty")));
            }
        }
        // An agent must have *something* to do: at least one skill or one topic.
        if self.skill_ids.is_empty() && self.data_scope.topics.is_empty() {
            return Err(AgentError(
                "agent must declare at least one skill or one research topic".into(),
            ));
        }
        if let Trigger::Interval { every_hours } = self.trigger {
            if every_hours == 0 {
                return Err(AgentError("interval every_hours must be >= 1".into()));
            }
        }
        if let Trigger::OnReferenceUpdate { source } = &self.trigger {
            if source.trim().is_empty() {
                return Err(AgentError("onReferenceUpdate.source must not be empty".into()));
            }
        }
        Ok(())
    }

    /// The set of rsIDs this definition explicitly names (public identifiers).
    /// Used by the privacy ledger and egress guard to bound what may be read.
    pub fn scoped_rsids(&self) -> &[String] {
        &self.data_scope.rsids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> AgentDefinition {
        AgentDefinition {
            schema_version: 1,
            id: "org.sovereigndna.agents.demo".into(),
            version: "1.0.0".into(),
            name: "Demo".into(),
            description: "d".into(),
            skill_ids: vec!["org.sovereigndna.traits.core".into()],
            data_scope: DataScope::default(),
            llm: LlmConfig::default(),
            trigger: Trigger::Manual,
            template_id: None,
            instructions: String::new(),
            disclaimer: "not medical advice".into(),
        }
    }

    #[test]
    fn valid_definition_passes() {
        assert!(minimal().validate().is_ok());
    }

    #[test]
    fn default_llm_is_local_and_not_remote() {
        assert_eq!(LlmConfig::default(), LlmConfig::LocalOllama { model: None });
        assert!(!LlmConfig::default().is_remote());
        assert!(LlmConfig::Claude { model: None }.is_remote());
        assert!(!LlmConfig::None.is_remote());
    }

    #[test]
    fn rejects_future_schema_version() {
        let mut m = minimal();
        m.schema_version = AGENT_SCHEMA_VERSION + 1;
        assert!(m.validate().is_err());
    }

    #[test]
    fn rejects_zero_schema_version() {
        let mut m = minimal();
        m.schema_version = 0;
        assert!(m.validate().is_err());
    }

    #[test]
    fn rejects_empty_disclaimer() {
        let mut m = minimal();
        m.disclaimer = "  ".into();
        assert!(m.validate().is_err());
    }

    #[test]
    fn rejects_agent_with_nothing_to_do() {
        let mut m = minimal();
        m.skill_ids.clear();
        m.data_scope.topics.clear();
        assert!(m.validate().is_err());
    }

    #[test]
    fn topic_only_agent_is_valid() {
        let mut m = minimal();
        m.skill_ids.clear();
        m.data_scope.topics = vec!["APOE".into()];
        assert!(m.validate().is_ok());
    }

    #[test]
    fn rejects_zero_interval() {
        let mut m = minimal();
        m.trigger = Trigger::Interval { every_hours: 0 };
        assert!(m.validate().is_err());
        m.trigger = Trigger::Interval { every_hours: 24 };
        assert!(m.validate().is_ok());
    }

    #[test]
    fn data_scope_rsid_matching_is_case_insensitive() {
        let scope = DataScope {
            rsids: vec!["rs429358".into()],
            topics: vec![],
        };
        assert!(scope.allows_rsid("RS429358"));
        assert!(!scope.allows_rsid("rs7412"));
        assert!(!scope.is_rsid_unrestricted());
        // unrestricted scope allows anything
        assert!(DataScope::default().allows_rsid("rsAnything"));
    }

    #[test]
    fn round_trips_through_json() {
        let m = minimal();
        let json = serde_json::to_string(&m).unwrap();
        let back = AgentDefinition::from_json(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn trigger_tags_serialize_as_expected() {
        let j = serde_json::to_string(&Trigger::Interval { every_hours: 12 }).unwrap();
        assert!(j.contains("\"kind\":\"interval\""));
        assert!(j.contains("\"every_hours\":12") || j.contains("\"everyHours\":12"));
    }
}
