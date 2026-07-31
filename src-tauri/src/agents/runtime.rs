//! Agent run loop (Phase 3.3).
//!
//! Composes an [`AgentDefinition`] into one execution: run each declared skill
//! over the genome (via the Phase-2 engine), turn the matched results into
//! scoped [`AgentFinding`]s, append them to the agent's [`AgentMemory`], and
//! optionally ask a [`Summarizer`] (the LLM step) to write a digest note.
//!
//! Everything the loop touches is behind a trait so it is fully unit-testable
//! without a database, a network, or a real LLM:
//! * [`SkillRunner`] — "run skill X over the genome" (real impl = Phase-2
//!   registry + SQLite genome; test impl = canned outputs);
//! * [`AgentMemory`] — the findings log;
//! * [`Summarizer`] — the optional LLM summarisation step (real impl honors
//!   [`crate::agents::definition::LlmConfig`] + egress rules; test impl is a stub);
//! * [`Clock`] — timestamps (deterministic in tests).
//!
//! **Privacy invariant:** the loop only ever reads rsIDs the definition scopes
//! and records public rsIDs in findings — no genotype leaves this function. The
//! per-action privacy ledger (Phase 3.7) wraps this to make that auditable.

use serde::{Deserialize, Serialize};

use super::definition::AgentDefinition;
use super::memory::{AgentFinding, AgentMemory, FindingKind};
use crate::skills::engine::SkillOutput;

/// Runs a skill (by id) over the current genome.
pub trait SkillRunner {
    /// Execute skill `skill_id`, returning its [`SkillOutput`] or an error
    /// message. Missing/untrusted skills should return `Err`.
    fn run_skill(&self, skill_id: &str) -> Result<SkillOutput, String>;
}

/// The optional LLM summarisation step.
pub trait Summarizer {
    /// Summarise a run's findings into a short note, or `None` to skip.
    ///
    /// The implementation is responsible for honoring the definition's
    /// [`LlmConfig`](super::definition::LlmConfig) and the egress rules — the run
    /// loop treats it as an opaque, best-effort enrichment.
    fn summarize(&self, def: &AgentDefinition, findings: &[AgentFinding]) -> Option<String>;
}

/// A summariser that never runs an LLM (used for `LlmConfig::None` and tests).
pub struct NoSummarizer;
impl Summarizer for NoSummarizer {
    fn summarize(&self, _def: &AgentDefinition, _findings: &[AgentFinding]) -> Option<String> {
        None
    }
}

/// Source of timestamps, so runs are deterministic under test.
pub trait Clock {
    fn now_rfc3339(&self) -> String;
}

/// A fixed clock for tests.
pub struct FixedClock(pub String);
impl Clock for FixedClock {
    fn now_rfc3339(&self) -> String {
        self.0.clone()
    }
}

/// Outcome of one agent run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Ran to completion (possibly with zero findings).
    Completed,
    /// Some skills errored but the run still finished with partial results.
    Partial,
    /// The definition was invalid; nothing ran.
    Failed,
}

/// A full record of one agent run — persisted as run history (Phase 3.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRun {
    pub run_id: String,
    pub agent_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub status: RunStatus,
    /// Findings produced this run (also appended to memory).
    pub findings: Vec<AgentFinding>,
    /// Optional LLM digest note.
    pub summary: Option<String>,
    /// Human-readable log of what happened (skills run, errors, counts).
    pub log: Vec<String>,
}

/// Execute one agent run.
///
/// Steps:
/// 1. validate the definition (invalid → [`RunStatus::Failed`], nothing runs);
/// 2. run each declared skill; convert findings, keeping only those whose
///    contributing rsIDs fall within the data scope;
/// 3. append findings to memory and record the run timestamp;
/// 4. ask the summariser for an optional note;
/// 5. return the [`AgentRun`].
pub fn run_agent<S, M, Z, C>(
    def: &AgentDefinition,
    run_id: &str,
    skills: &S,
    memory: &mut M,
    summarizer: &Z,
    clock: &C,
) -> AgentRun
where
    S: SkillRunner,
    M: AgentMemory,
    Z: Summarizer,
    C: Clock,
{
    let started_at = clock.now_rfc3339();
    let mut log: Vec<String> = Vec::new();

    if let Err(e) = def.validate() {
        let finished_at = clock.now_rfc3339();
        log.push(format!("definition invalid: {e}"));
        return AgentRun {
            run_id: run_id.to_string(),
            agent_id: def.id.clone(),
            started_at,
            finished_at,
            status: RunStatus::Failed,
            findings: Vec::new(),
            summary: None,
            log,
        };
    }

    let mut findings: Vec<AgentFinding> = Vec::new();
    let mut ordinal: usize = 0;
    let mut had_error = false;
    let created_at = clock.now_rfc3339();

    for skill_id in &def.skill_ids {
        match skills.run_skill(skill_id) {
            Ok(output) => {
                let produced =
                    findings_from_skill(def, run_id, skill_id, &output, &mut ordinal, &created_at);
                log.push(format!(
                    "skill '{skill_id}': {} in-scope finding(s)",
                    produced.len()
                ));
                findings.extend(produced);
            }
            Err(e) => {
                had_error = true;
                log.push(format!("skill '{skill_id}' failed: {e}"));
            }
        }
    }

    // Persist findings + record the run.
    for f in &findings {
        memory.append(f.clone());
    }
    memory.record_run(&def.id, &created_at);

    // Optional LLM summarisation step.
    let summary = summarizer.summarize(def, &findings);
    if let Some(s) = &summary {
        log.push(format!("summary: {} char note", s.len()));
    }

    let status = if had_error {
        RunStatus::Partial
    } else {
        RunStatus::Completed
    };
    let finished_at = clock.now_rfc3339();

    AgentRun {
        run_id: run_id.to_string(),
        agent_id: def.id.clone(),
        started_at,
        finished_at,
        status,
        findings,
        summary,
        log,
    }
}

/// Convert a skill's output into scoped findings.
///
/// A finding is kept only if at least one contributing rsID is within the
/// agent's data scope (an unrestricted scope keeps everything the skill produced).
fn findings_from_skill(
    def: &AgentDefinition,
    run_id: &str,
    skill_id: &str,
    output: &SkillOutput,
    ordinal: &mut usize,
    created_at: &str,
) -> Vec<AgentFinding> {
    let mut out = Vec::new();
    for f in &output.findings {
        let rsids: Vec<String> = f.contributing.iter().map(|c| c.rsid.clone()).collect();
        let in_scope = def.data_scope.is_rsid_unrestricted()
            || rsids.iter().any(|r| def.data_scope.allows_rsid(r));
        if !in_scope {
            continue;
        }
        let detail = format!(
            "{} — {} (confidence {:.2}) [{}]",
            f.category, f.prediction, f.confidence, skill_id
        );
        out.push(AgentFinding::new(
            &def.id,
            run_id,
            *ordinal,
            FindingKind::SkillResult,
            f.name.clone(),
            detail,
            rsids,
            created_at,
        ));
        *ordinal += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::definition::{DataScope, LlmConfig, Trigger};
    use crate::agents::memory::InMemoryMemory;
    use crate::skills::engine::{ContributingVariant, Finding};
    use crate::skills::manifest::EvidenceTier;
    use std::collections::HashMap;

    fn def(skills: Vec<&str>, scope_rsids: Vec<&str>) -> AgentDefinition {
        AgentDefinition {
            schema_version: 1,
            id: "org.sovereigndna.agents.test".into(),
            version: "1.0.0".into(),
            name: "Test Agent".into(),
            description: "d".into(),
            skill_ids: skills.into_iter().map(String::from).collect(),
            data_scope: DataScope {
                rsids: scope_rsids.into_iter().map(String::from).collect(),
                topics: vec![],
            },
            llm: LlmConfig::None,
            trigger: Trigger::Manual,
            template_id: None,
            instructions: String::new(),
            disclaimer: "not medical advice".into(),
        }
    }

    fn skill_output(skill_id: &str, findings: Vec<Finding>) -> SkillOutput {
        SkillOutput {
            skill_id: skill_id.into(),
            skill_version: "1.0.0".into(),
            evidence_tier: EvidenceTier::Community,
            disclaimer: "d".into(),
            citations: vec![],
            findings,
        }
    }

    fn finding(name: &str, rsid: &str) -> Finding {
        Finding {
            name: name.into(),
            category: "Cat".into(),
            prediction: "pred".into(),
            confidence: 0.7,
            description: "desc".into(),
            contributing: vec![ContributingVariant {
                rsid: rsid.into(),
                genotype: "AA".into(),
                effect: "e".into(),
            }],
            population_frequency: None,
        }
    }

    struct CannedRunner(HashMap<String, SkillOutput>);
    impl SkillRunner for CannedRunner {
        fn run_skill(&self, skill_id: &str) -> Result<SkillOutput, String> {
            self.0
                .get(skill_id)
                .cloned()
                .ok_or_else(|| format!("no such skill: {skill_id}"))
        }
    }

    fn runner(pairs: Vec<(&str, SkillOutput)>) -> CannedRunner {
        CannedRunner(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    #[test]
    fn completed_run_produces_and_persists_findings() {
        let d = def(vec!["s1"], vec![]);
        let out = skill_output("s1", vec![finding("Eye Color", "rs12913832")]);
        let r = runner(vec![("s1", out)]);
        let mut mem = InMemoryMemory::new();
        let run = run_agent(&d, "run-1", &r, &mut mem, &NoSummarizer, &FixedClock("2026-07-16T00:00:00Z".into()));
        assert_eq!(run.status, RunStatus::Completed);
        assert_eq!(run.findings.len(), 1);
        assert_eq!(run.findings[0].title, "Eye Color");
        // persisted to memory + run recorded
        assert_eq!(mem.findings(&d.id).len(), 1);
        assert_eq!(mem.last_run_at(&d.id).as_deref(), Some("2026-07-16T00:00:00Z"));
    }

    #[test]
    fn scope_filters_out_of_scope_findings() {
        // scope pins rs1 only; skill produces one in-scope + one out-of-scope
        let d = def(vec!["s1"], vec!["rs1"]);
        let out = skill_output(
            "s1",
            vec![finding("In", "rs1"), finding("Out", "rs999")],
        );
        let r = runner(vec![("s1", out)]);
        let mut mem = InMemoryMemory::new();
        let run = run_agent(&d, "run-2", &r, &mut mem, &NoSummarizer, &FixedClock("2026-07-16T00:00:00Z".into()));
        assert_eq!(run.findings.len(), 1);
        assert_eq!(run.findings[0].title, "In");
    }

    #[test]
    fn missing_skill_marks_run_partial() {
        let d = def(vec!["s1", "missing"], vec![]);
        let out = skill_output("s1", vec![finding("X", "rs1")]);
        let r = runner(vec![("s1", out)]);
        let mut mem = InMemoryMemory::new();
        let run = run_agent(&d, "run-3", &r, &mut mem, &NoSummarizer, &FixedClock("2026-07-16T00:00:00Z".into()));
        assert_eq!(run.status, RunStatus::Partial);
        assert_eq!(run.findings.len(), 1);
        assert!(run.log.iter().any(|l| l.contains("failed")));
    }

    #[test]
    fn invalid_definition_fails_without_running() {
        let mut d = def(vec!["s1"], vec![]);
        d.disclaimer = String::new(); // invalid
        let out = skill_output("s1", vec![finding("X", "rs1")]);
        let r = runner(vec![("s1", out)]);
        let mut mem = InMemoryMemory::new();
        let run = run_agent(&d, "run-4", &r, &mut mem, &NoSummarizer, &FixedClock("2026-07-16T00:00:00Z".into()));
        assert_eq!(run.status, RunStatus::Failed);
        assert!(run.findings.is_empty());
        assert!(mem.is_empty());
    }

    #[test]
    fn summarizer_note_is_attached() {
        struct Stub;
        impl Summarizer for Stub {
            fn summarize(&self, _d: &AgentDefinition, f: &[AgentFinding]) -> Option<String> {
                Some(format!("{} findings", f.len()))
            }
        }
        let d = def(vec!["s1"], vec![]);
        let out = skill_output("s1", vec![finding("X", "rs1")]);
        let r = runner(vec![("s1", out)]);
        let mut mem = InMemoryMemory::new();
        let run = run_agent(&d, "run-5", &r, &mut mem, &Stub, &FixedClock("2026-07-16T00:00:00Z".into()));
        assert_eq!(run.summary.as_deref(), Some("1 findings"));
    }
}
