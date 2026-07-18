//! User-created agents (Phase 3).
//!
//! An **agent** is a saved, reusable analysis worker the user composes from the
//! building blocks the earlier phases added:
//! * a **skill set** — skills (Phase 2) it runs each round;
//! * a **data scope** — which variants / research topics it may read;
//! * an **LLM** — local Ollama by default, Claude opt-in;
//! * persistent **memory** — its append-only findings log.
//!
//! Sub-phases in this module (spec §4):
//! * 3.1 — [`definition`] (agent schema) + [`store`] (SQLite persistence) +
//!   migration `008_agents.sql` (definitions / runs / findings tables).
//! * 3.3 — [`runtime`] (the run loop composing skills + scope + memory + LLM).
//!
//! The pure modules ([`definition`], [`memory`], [`runtime`]) are database- and
//! network-free and unit-tested with in-memory backends; [`store`] and the
//! [`commands`](crate::commands::agents) layer wire them to SQLite / Tauri.

pub mod definition;
pub mod ledger;
pub mod memory;
pub mod runtime;
pub mod safety;
pub mod scheduler;
pub mod sharing;
pub mod store;
pub mod templates;

use rusqlite::Connection;

use crate::skills::engine::SkillOutput;
use crate::skills::manifest::SkillManifest;
use crate::skills::{builtin_manifests, DbReferenceAvailability, SqliteGenotypeSource};
use definition::AgentDefinition;
use runtime::SkillRunner;

/// A [`SkillRunner`] that executes the app's **built-in** skills over a genome
/// loaded from SQLite.
///
/// Registry-installed skills (Phase 2.5/2.6) require the skill-registry app state
/// to be wired into Tauri first; until then an agent that names an uninstalled
/// skill degrades gracefully — that skill errors and the run is marked partial,
/// rather than the whole run failing.
pub struct BuiltinSkillRunner<'a> {
    genome: SqliteGenotypeSource,
    refs: DbReferenceAvailability<'a>,
    builtins: Vec<SkillManifest>,
}

impl<'a> BuiltinSkillRunner<'a> {
    /// Build a runner for `def` against `genome_id`, batch-loading only the rsIDs
    /// the agent's declared built-in skills touch.
    pub fn new(conn: &'a Connection, def: &AgentDefinition, genome_id: i64) -> Self {
        let builtins = builtin_manifests();
        let mut rsids: Vec<String> = Vec::new();
        for skill_id in &def.skill_ids {
            if let Some(m) = builtins.iter().find(|m| &m.id == skill_id) {
                for v in &m.variants {
                    rsids.push(v.rsid.clone());
                }
            }
        }
        rsids.sort();
        rsids.dedup();
        // Loading an empty rsID set returns early without touching the DB, so the
        // fallback is infallible — keeping this constructor total.
        let genome = SqliteGenotypeSource::load(conn, genome_id, &rsids)
            .or_else(|_| SqliteGenotypeSource::load(conn, genome_id, &[]))
            .expect("loading an empty genotype set is infallible");
        Self {
            genome,
            refs: DbReferenceAvailability::new(conn),
            builtins,
        }
    }
}

impl SkillRunner for BuiltinSkillRunner<'_> {
    fn run_skill(&self, skill_id: &str) -> Result<SkillOutput, String> {
        match self.builtins.iter().find(|m| m.id == skill_id) {
            Some(m) => crate::skills::engine::evaluate(m, &self.genome, &self.refs)
                .map_err(|e| e.to_string()),
            None => Err(format!(
                "skill '{skill_id}' is not a built-in and registry skills are not yet installed"
            )),
        }
    }
}

/// Summarise a completed run's findings with **local Ollama**, honoring the
/// definition's [`LlmConfig`](definition::LlmConfig).
///
/// Privacy: only the findings' human-readable text (already device-safe — public
/// rsIDs + labels, never genotypes) is sent to `localhost:11434`, which is local.
/// `LlmConfig::None` skips the call entirely; `Claude` is intentionally *not*
/// auto-invoked here (it egresses to Anthropic and reuses the streaming
/// `chat_with_claude` path on explicit user action) so a background agent never
/// silently makes a paid remote call.
pub async fn summarize_run_local(
    def: &AgentDefinition,
    findings: &[memory::AgentFinding],
) -> Option<String> {
    use definition::LlmConfig;
    let model = match &def.llm {
        LlmConfig::LocalOllama { model } => model.clone().unwrap_or_else(|| "llama3.2".to_string()),
        // Do not auto-call a paid/remote model from a background agent.
        LlmConfig::Claude { .. } | LlmConfig::None => return None,
    };
    if findings.is_empty() {
        return None;
    }

    let mut bullet = String::new();
    for f in findings.iter().take(25) {
        bullet.push_str(&format!("- {}: {}\n", f.title, f.detail));
    }
    let instructions = if def.instructions.trim().is_empty() {
        "Summarise these genomic-agent findings in 2-4 sentences for the user."
    } else {
        def.instructions.trim()
    };
    let prompt = format!(
        "{instructions}\n\nFindings:\n{bullet}\nRemember: educational only, not medical advice."
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .ok()?;
    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
    });
    let resp = client
        .post("http://localhost:11434/api/generate")
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: serde_json::Value = resp.json().await.ok()?;
    let text = v.get("response")?.as_str()?.trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}
