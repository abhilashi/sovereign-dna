//! Tauri commands for user-created agents (Phase 3.1 + 3.3).
//!
//! Thin glue: validate input, acquire the DB lock for short reads/writes (never
//! held across the network), and delegate to the pure agent [`runtime`] +
//! [`store`] adapter.

use tauri::State;

use crate::agents::definition::AgentDefinition;
use crate::agents::ledger::{
    self, ActionKind, ConsentGrant, ConsentScope, Egress, EgressSummary, LedgerEntry,
    ProposedAction,
};
use crate::agents::memory::{AgentFinding, AgentMemory};
use crate::agents::runtime::{run_agent, AgentRun, Clock, NoSummarizer};
use crate::agents::scheduler;
use crate::agents::store::{self, SqliteAgentMemory};
use crate::agents::{summarize_run_local, BuiltinSkillRunner};
use crate::db::Database;
use crate::error::AppError;

/// A real wall-clock [`Clock`] for production runs.
struct SystemClock;
impl Clock for SystemClock {
    fn now_rfc3339(&self) -> String {
        chrono::Utc::now().to_rfc3339()
    }
}

/// Create or update an agent definition. The definition must validate and, by
/// construction, contains no genome data.
#[tauri::command]
pub fn save_agent(agent: AgentDefinition, db: State<'_, Database>) -> Result<(), AppError> {
    agent.validate().map_err(|e| AppError::Analysis(e.to_string()))?;
    let conn = db
        .0
        .lock()
        .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
    store::upsert_definition(&conn, &agent)
}

/// List all saved agent definitions.
#[tauri::command]
pub fn list_agents(db: State<'_, Database>) -> Result<Vec<AgentDefinition>, AppError> {
    let conn = db
        .0
        .lock()
        .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
    store::list_definitions(&conn)
}

/// Fetch one agent definition by id.
#[tauri::command]
pub fn get_agent(agent_id: String, db: State<'_, Database>) -> Result<AgentDefinition, AppError> {
    let conn = db
        .0
        .lock()
        .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
    store::get_definition(&conn, &agent_id)
}

/// Delete an agent (cascades to its runs + findings).
#[tauri::command]
pub fn delete_agent(agent_id: String, db: State<'_, Database>) -> Result<(), AppError> {
    let conn = db
        .0
        .lock()
        .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
    store::delete_definition(&conn, &agent_id)
}

/// Run an agent once, now, against `genome_id`.
///
/// The synchronous skill pass (DB-only, no network) runs under a short lock and
/// persists findings + the run row. The optional **local** LLM summarisation
/// happens afterwards without holding the lock, then patches the run's summary.
#[tauri::command]
pub async fn run_agent_now(
    agent_id: String,
    genome_id: i64,
    db: State<'_, Database>,
) -> Result<AgentRun, AppError> {
    execute_agent(&db, &agent_id, genome_id).await
}

/// Core execution shared by [`run_agent_now`] and [`run_due_agents`].
///
/// Phase 1 (sync, under the DB lock): run the agent's skills, persist findings +
/// the run row, and record the on-device read in the privacy ledger. Phase 2 (no
/// lock): optional local-only LLM summarisation, ledgered for transparency.
async fn execute_agent(
    db: &Database,
    agent_id: &str,
    genome_id: i64,
) -> Result<AgentRun, AppError> {
    // Deterministic-enough run id (agent + timestamp).
    let run_id = format!(
        "run-{}-{}",
        agent_id.replace(|c: char| !c.is_ascii_alphanumeric(), "-"),
        chrono::Utc::now().timestamp_millis()
    );

    // ── Phase 1: synchronous skill pass under the DB lock ──────────────
    let (mut run, def) = {
        let conn = db
            .0
            .lock()
            .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
        let def = store::get_definition(&conn, agent_id)?;
        let runner = BuiltinSkillRunner::new(&conn, &def, genome_id);
        let mut memory = SqliteAgentMemory::new(&conn);
        let run = run_agent(
            &def,
            &run_id,
            &runner,
            &mut memory,
            &NoSummarizer,
            &SystemClock,
        );
        store::insert_run(&conn, &run, genome_id)?;

        // Privacy ledger (3.7): record the on-device variant read this run made.
        // Reading genotypes locally has no egress → always AllowedLocal, but it
        // is still logged so the audit trail is complete.
        let mut rsids: Vec<String> = run
            .findings
            .iter()
            .flat_map(|f| f.rsids.clone())
            .collect();
        rsids.sort();
        rsids.dedup();
        let read_action = ProposedAction {
            agent_id: def.id.clone(),
            run_id: run_id.clone(),
            kind: ActionKind::ReadVariants,
            rsids,
            egress: None,
            description: format!(
                "read {} skill(s) locally; {} finding(s)",
                def.skill_ids.len(),
                run.findings.len()
            ),
        };
        let entry = ledger::record(read_action, &[], &run.started_at);
        let _ = store::append_ledger(&conn, &entry);
        (run, def)
    };

    // ── Phase 2: optional local-only summarisation (no lock held) ──────
    if let Some(summary) = summarize_run_local(&def, &run.findings).await {
        let conn = db
            .0
            .lock()
            .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
        store::set_run_summary(&conn, &run_id, &summary)?;
        // Ledger the local-LLM summarisation (localhost → local egress, no consent
        // required, but recorded for transparency).
        let llm_action = ProposedAction {
            agent_id: run.agent_id.clone(),
            run_id: run_id.clone(),
            kind: ActionKind::LlmLocal,
            rsids: vec![],
            egress: Some(Egress {
                endpoint: "localhost:11434".into(),
                is_local: true,
                identifiers: vec![],
                description: "Ollama summary of findings (on-device)".into(),
            }),
            description: "summarised run findings with local Ollama".into(),
        };
        let entry = ledger::record(llm_action, &[], &chrono::Utc::now().to_rfc3339());
        let _ = store::append_ledger(&conn, &entry);
        run.summary = Some(summary);
    }

    Ok(run)
}

// ── Privacy & consent ledger commands (Phase 3.7) ─────────────────────

/// Grant an agent explicit, revocable consent to perform a class of actions.
#[tauri::command]
pub fn grant_consent(
    agent_id: String,
    scope: ConsentScope,
    note: Option<String>,
    db: State<'_, Database>,
) -> Result<ConsentGrant, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let grant = ConsentGrant {
        id: format!("consent-{}", chrono::Utc::now().timestamp_millis()),
        agent_id,
        scope,
        granted_at: now,
        revoked_at: None,
        note: note.unwrap_or_default(),
    };
    let conn = db
        .0
        .lock()
        .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
    store::upsert_consent(&conn, &grant)?;
    Ok(grant)
}

/// Revoke a consent grant. Future actions requiring it are denied; historical
/// ledger entries are untouched.
#[tauri::command]
pub fn revoke_consent(consent_id: String, db: State<'_, Database>) -> Result<(), AppError> {
    let conn = db
        .0
        .lock()
        .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
    store::revoke_consent(&conn, &consent_id, &chrono::Utc::now().to_rfc3339())
}

/// List an agent's consent grants (active only by default).
#[tauri::command]
pub fn list_consents(
    agent_id: String,
    include_revoked: Option<bool>,
    db: State<'_, Database>,
) -> Result<Vec<ConsentGrant>, AppError> {
    let conn = db
        .0
        .lock()
        .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
    store::list_consents(&conn, &agent_id, !include_revoked.unwrap_or(false))
}

/// The per-action privacy ledger for an agent (newest first).
#[tauri::command]
pub fn get_agent_ledger(
    agent_id: String,
    limit: Option<i64>,
    db: State<'_, Database>,
) -> Result<Vec<LedgerEntry>, AppError> {
    let conn = db
        .0
        .lock()
        .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
    store::list_ledger(&conn, &agent_id, limit.unwrap_or(200))
}

/// A user-facing summary of exactly what an agent has sent off the device.
#[tauri::command]
pub fn get_agent_egress_summary(
    agent_id: String,
    db: State<'_, Database>,
) -> Result<EgressSummary, AppError> {
    let conn = db
        .0
        .lock()
        .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
    let entries = store::list_ledger(&conn, &agent_id, 10_000)?;
    Ok(ledger::summarize_egress(&entries))
}

/// List an agent's findings (its persistent memory), newest first.
#[tauri::command]
pub fn get_agent_findings(
    agent_id: String,
    db: State<'_, Database>,
) -> Result<Vec<AgentFinding>, AppError> {
    let conn = db
        .0
        .lock()
        .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
    let memory = SqliteAgentMemory::new(&conn);
    Ok(memory.findings(&agent_id))
}

/// Recent run history for an agent.
#[tauri::command]
pub fn get_agent_runs(
    agent_id: String,
    limit: Option<i64>,
    db: State<'_, Database>,
) -> Result<Vec<AgentRun>, AppError> {
    let conn = db
        .0
        .lock()
        .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
    store::list_runs(&conn, &agent_id, limit.unwrap_or(20))
}

/// Number of unseen findings for an agent (drives the "new" badge).
#[tauri::command]
pub fn get_agent_unseen_count(
    agent_id: String,
    db: State<'_, Database>,
) -> Result<usize, AppError> {
    let conn = db
        .0
        .lock()
        .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
    let memory = SqliteAgentMemory::new(&conn);
    Ok(memory.unseen_count(&agent_id))
}

/// Mark a finding as seen.
#[tauri::command]
pub fn mark_agent_finding_seen(
    finding_id: String,
    db: State<'_, Database>,
) -> Result<bool, AppError> {
    let conn = db
        .0
        .lock()
        .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
    let mut memory = SqliteAgentMemory::new(&conn);
    Ok(memory.mark_seen(&finding_id))
}

// ── Scheduler + safety commands (Phase 3.2 + 3.6) ─────────────────────

/// Record a fleet event the scheduler reacts to (e.g. a reference DB finished
/// updating). Call this from the reference-download / research-scan flows.
#[tauri::command]
pub fn record_agent_event(
    kind: String,
    source: Option<String>,
    db: State<'_, Database>,
) -> Result<(), AppError> {
    let conn = db
        .0
        .lock()
        .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
    store::record_event(&conn, &kind, source.as_deref(), &chrono::Utc::now().to_rfc3339())
}

/// Ids of agents currently due to run (time + event triggers evaluated against
/// each agent's last run and recent fleet events).
#[tauri::command]
pub fn list_due_agents(db: State<'_, Database>) -> Result<Vec<String>, AppError> {
    let conn = db
        .0
        .lock()
        .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
    let with_last = store::definitions_with_last_run(&conn)?;
    let events = store::list_recent_events(&conn, 500)?;
    let now = chrono::Utc::now().to_rfc3339();
    Ok(scheduler::due_agents(&with_last, &now, &events))
}

/// The next scheduled run time for an agent (interval triggers only).
#[tauri::command]
pub fn get_agent_next_run(
    agent_id: String,
    db: State<'_, Database>,
) -> Result<Option<String>, AppError> {
    let conn = db
        .0
        .lock()
        .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
    let def = store::get_definition(&conn, &agent_id)?;
    let last = SqliteAgentMemory::new(&conn).last_run_at(&agent_id);
    let now = chrono::Utc::now().to_rfc3339();
    Ok(scheduler::next_run_at(&def.trigger, last.as_deref(), &now))
}

/// Run every agent that is currently due, once, against `genome_id`. This is the
/// scheduler's execution entry point; a background tick (Phase 3.8) calls it.
#[tauri::command]
pub async fn run_due_agents(
    genome_id: i64,
    db: State<'_, Database>,
) -> Result<Vec<AgentRun>, AppError> {
    let due = {
        let conn = db
            .0
            .lock()
            .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
        let with_last = store::definitions_with_last_run(&conn)?;
        let events = store::list_recent_events(&conn, 500)?;
        let now = chrono::Utc::now().to_rfc3339();
        scheduler::due_agents(&with_last, &now, &events)
    };
    let mut runs = Vec::with_capacity(due.len());
    for agent_id in due {
        // A single failing agent must not abort the whole batch.
        if let Ok(run) = execute_agent(&db, &agent_id, genome_id).await {
            runs.push(run);
        }
    }
    Ok(runs)
}

/// Rust-layer egress preflight (Phase 3.6): reject an outbound call whose
/// endpoint is not allowlisted or whose identifiers are not public rsIDs — the
/// backend-side complement to the webview CSP.
#[tauri::command]
pub fn preflight_egress(endpoint: String, identifiers: Vec<String>) -> Result<(), AppError> {
    crate::agents::safety::EgressGuard::default()
        .check(&endpoint, &identifiers)
        .map_err(AppError::Analysis)
}
