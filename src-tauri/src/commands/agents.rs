//! Tauri commands for user-created agents (Phase 3.1 + 3.3).
//!
//! Thin glue: validate input, acquire the DB lock for short reads/writes (never
//! held across the network), and delegate to the pure agent [`runtime`] +
//! [`store`] adapter.

use tauri::State;

use crate::agents::definition::AgentDefinition;
use crate::agents::memory::{AgentFinding, AgentMemory};
use crate::agents::runtime::{run_agent, AgentRun, Clock, NoSummarizer};
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
        let def = store::get_definition(&conn, &agent_id)?;
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
        (run, def)
    };

    // ── Phase 2: optional local-only summarisation (no lock held) ──────
    if let Some(summary) = summarize_run_local(&def, &run.findings).await {
        let conn = db
            .0
            .lock()
            .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {e}")))?;
        store::set_run_summary(&conn, &run_id, &summary)?;
        run.summary = Some(summary);
    }

    Ok(run)
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
