//! SQLite persistence for agents (Phase 3.1) — real-crate only.
//!
//! Kept out of the pure [`definition`](super::definition) /
//! [`memory`](super::memory) / [`runtime`](super::runtime) modules (which are
//! database-free and unit-tested with an in-memory backend) so the agent logic
//! stays trivially testable. This module wires those types to the `agent_*`
//! tables from migration 006.
//!
//! `rusqlite::Connection::execute` takes `&self` (interior mutability), so the
//! reference-availability adapter and this memory adapter can both hold a shared
//! `&Connection` at the same time during a run.

use rusqlite::Connection;

use super::definition::AgentDefinition;
use super::ledger::{ConsentGrant, LedgerEntry};
use super::memory::{AgentFinding, AgentMemory, FindingKind};
use super::runtime::AgentRun;
use super::scheduler::FleetEvent;
use crate::error::AppError;

fn kind_str(k: FindingKind) -> &'static str {
    match k {
        FindingKind::SkillResult => "skill_result",
        FindingKind::ResearchArticle => "research_article",
        FindingKind::Note => "note",
    }
}

fn kind_from_str(s: &str) -> FindingKind {
    match s {
        "research_article" => FindingKind::ResearchArticle,
        "note" => FindingKind::Note,
        _ => FindingKind::SkillResult,
    }
}

/// Insert (or replace) an agent definition. The stored JSON is the full,
/// validated definition — which by construction contains no genome data.
pub fn upsert_definition(conn: &Connection, def: &AgentDefinition) -> Result<(), AppError> {
    def.validate().map_err(|e| AppError::Analysis(e.to_string()))?;
    let json = serde_json::to_string(def)?;
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO agent_definitions (id, version, name, definition_json, template_id, enabled, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6)
         ON CONFLICT(id) DO UPDATE SET
            version = excluded.version,
            name = excluded.name,
            definition_json = excluded.definition_json,
            template_id = excluded.template_id,
            updated_at = excluded.updated_at",
        rusqlite::params![
            def.id,
            def.version,
            def.name,
            json,
            def.template_id,
            now
        ],
    )?;
    Ok(())
}

/// List all stored agent definitions (newest updated first).
pub fn list_definitions(conn: &Connection) -> Result<Vec<AgentDefinition>, AppError> {
    let mut stmt = conn
        .prepare("SELECT definition_json FROM agent_definitions ORDER BY updated_at DESC")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for r in rows {
        let json = r?;
        if let Ok(def) = AgentDefinition::from_json(&json) {
            out.push(def);
        }
    }
    Ok(out)
}

/// Load one agent definition by id.
pub fn get_definition(conn: &Connection, agent_id: &str) -> Result<AgentDefinition, AppError> {
    let json: String = conn
        .query_row(
            "SELECT definition_json FROM agent_definitions WHERE id = ?1",
            [agent_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::NotFound(format!("agent not found: {agent_id}")))?;
    AgentDefinition::from_json(&json).map_err(|e| AppError::Analysis(e.to_string()))
}

/// Delete an agent and (via cascade) its runs + findings.
pub fn delete_definition(conn: &Connection, agent_id: &str) -> Result<(), AppError> {
    let n = conn.execute("DELETE FROM agent_definitions WHERE id = ?1", [agent_id])?;
    if n == 0 {
        return Err(AppError::NotFound(format!("agent not found: {agent_id}")));
    }
    Ok(())
}

/// Persist a completed run (its findings are written separately via
/// [`SqliteAgentMemory::append`] during the run loop).
pub fn insert_run(
    conn: &Connection,
    run: &AgentRun,
    genome_id: i64,
) -> Result<(), AppError> {
    let status = match run.status {
        super::runtime::RunStatus::Completed => "completed",
        super::runtime::RunStatus::Partial => "partial",
        super::runtime::RunStatus::Failed => "failed",
    };
    let log_json = serde_json::to_string(&run.log)?;
    conn.execute(
        "INSERT OR REPLACE INTO agent_runs
            (run_id, agent_id, genome_id, started_at, finished_at, status, summary, log_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            run.run_id,
            run.agent_id,
            genome_id,
            run.started_at,
            run.finished_at,
            status,
            run.summary,
            log_json
        ],
    )?;
    Ok(())
}

/// List recent runs for an agent (newest first).
pub fn list_runs(conn: &Connection, agent_id: &str, limit: i64) -> Result<Vec<AgentRun>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT run_id, agent_id, started_at, finished_at, status, summary, log_json
         FROM agent_runs WHERE agent_id = ?1 ORDER BY started_at DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![agent_id, limit], |row| {
        let status_str: String = row.get(4)?;
        let log_json: String = row.get(6)?;
        Ok(AgentRun {
            run_id: row.get(0)?,
            agent_id: row.get(1)?,
            started_at: row.get(2)?,
            finished_at: row.get(3)?,
            status: match status_str.as_str() {
                "partial" => super::runtime::RunStatus::Partial,
                "failed" => super::runtime::RunStatus::Failed,
                _ => super::runtime::RunStatus::Completed,
            },
            findings: Vec::new(),
            summary: row.get(5)?,
            log: serde_json::from_str(&log_json).unwrap_or_default(),
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Update the summary column of a run after an async LLM step.
pub fn set_run_summary(conn: &Connection, run_id: &str, summary: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE agent_runs SET summary = ?2 WHERE run_id = ?1",
        rusqlite::params![run_id, summary],
    )?;
    Ok(())
}

// ── Privacy & consent ledger (Phase 3.7) ──────────────────────────────

/// Create or update a consent grant (by id).
pub fn upsert_consent(conn: &Connection, grant: &ConsentGrant) -> Result<(), AppError> {
    let scope_json = serde_json::to_string(&grant.scope)?;
    conn.execute(
        "INSERT INTO agent_consents (id, agent_id, scope_json, granted_at, revoked_at, note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
            scope_json = excluded.scope_json,
            revoked_at = excluded.revoked_at,
            note = excluded.note",
        rusqlite::params![
            grant.id,
            grant.agent_id,
            scope_json,
            grant.granted_at,
            grant.revoked_at,
            grant.note
        ],
    )?;
    Ok(())
}

/// Revoke a consent grant at `revoked_at`. Historical ledger entries keep their
/// original authorisation; only future actions are affected.
pub fn revoke_consent(
    conn: &Connection,
    consent_id: &str,
    revoked_at: &str,
) -> Result<(), AppError> {
    let n = conn.execute(
        "UPDATE agent_consents SET revoked_at = ?2 WHERE id = ?1 AND revoked_at IS NULL",
        rusqlite::params![consent_id, revoked_at],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!(
            "no active consent to revoke: {consent_id}"
        )));
    }
    Ok(())
}

/// List consent grants for an agent. When `active_only`, revoked grants are
/// omitted.
pub fn list_consents(
    conn: &Connection,
    agent_id: &str,
    active_only: bool,
) -> Result<Vec<ConsentGrant>, AppError> {
    let sql = if active_only {
        "SELECT id, agent_id, scope_json, granted_at, revoked_at, note
         FROM agent_consents WHERE agent_id = ?1 AND revoked_at IS NULL
         ORDER BY granted_at DESC"
    } else {
        "SELECT id, agent_id, scope_json, granted_at, revoked_at, note
         FROM agent_consents WHERE agent_id = ?1 ORDER BY granted_at DESC"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([agent_id], |row| {
        let scope_json: String = row.get(2)?;
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            scope_json,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (id, agent_id, scope_json, granted_at, revoked_at, note) = r?;
        let scope = serde_json::from_str(&scope_json)?;
        out.push(ConsentGrant {
            id,
            agent_id,
            scope,
            granted_at,
            revoked_at,
            note,
        });
    }
    Ok(out)
}

/// Append an immutable entry to the audit ledger.
pub fn append_ledger(conn: &Connection, entry: &LedgerEntry) -> Result<(), AppError> {
    let rsids_json = serde_json::to_string(&entry.rsids)?;
    let egress_json = match &entry.egress {
        Some(e) => Some(serde_json::to_string(e)?),
        None => None,
    };
    let outcome_json = serde_json::to_string(&entry.outcome)?;
    conn.execute(
        "INSERT OR REPLACE INTO agent_ledger
            (id, agent_id, run_id, timestamp, kind, rsids_json, egress_json, outcome_json, description)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            entry.id,
            entry.agent_id,
            entry.run_id,
            entry.timestamp,
            serde_json::to_string(&entry.kind)?.trim_matches('"'),
            rsids_json,
            egress_json,
            outcome_json,
            entry.description
        ],
    )?;
    Ok(())
}

/// Read the audit ledger for an agent (newest first).
pub fn list_ledger(
    conn: &Connection,
    agent_id: &str,
    limit: i64,
) -> Result<Vec<LedgerEntry>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, agent_id, run_id, timestamp, kind, rsids_json, egress_json, outcome_json, description
         FROM agent_ledger WHERE agent_id = ?1 ORDER BY timestamp DESC, id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![agent_id, limit], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
        ))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (id, agent_id, run_id, timestamp, kind_s, rsids_json, egress_json, outcome_json, description) =
            r?;
        // `kind` is stored unquoted; re-quote for serde enum parsing.
        let kind = serde_json::from_str(&format!("\"{kind_s}\""))
            .map_err(|e| AppError::Parse(e.to_string()))?;
        let egress = match egress_json {
            Some(j) => Some(serde_json::from_str(&j)?),
            None => None,
        };
        out.push(LedgerEntry {
            id,
            agent_id,
            run_id,
            timestamp,
            kind,
            rsids: serde_json::from_str(&rsids_json).unwrap_or_default(),
            egress,
            outcome: serde_json::from_str(&outcome_json)?,
            description,
        });
    }
    Ok(out)
}

// ── Fleet events + scheduling helpers (Phase 3.2) ─────────────────────

/// Record a fleet event the scheduler can react to (`kind` = `reference_updated`
/// / `new_matched_article`).
pub fn record_event(
    conn: &Connection,
    kind: &str,
    source: Option<&str>,
    at: &str,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO agent_events (kind, source, at) VALUES (?1, ?2, ?3)",
        rusqlite::params![kind, source, at],
    )?;
    Ok(())
}

/// The most recent fleet events (newest first), as [`FleetEvent`]s.
pub fn list_recent_events(conn: &Connection, limit: i64) -> Result<Vec<FleetEvent>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT kind, source, at FROM agent_events ORDER BY at DESC, id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (kind, source, at) = r?;
        match kind.as_str() {
            "reference_updated" => out.push(FleetEvent::ReferenceUpdated {
                source: source.unwrap_or_default(),
                at,
            }),
            "new_matched_article" => out.push(FleetEvent::NewMatchedArticle { at }),
            _ => {}
        }
    }
    Ok(out)
}

/// All enabled agent definitions paired with their last-run timestamp — the
/// input the scheduler needs to decide which agents are due.
pub fn definitions_with_last_run(
    conn: &Connection,
) -> Result<Vec<(AgentDefinition, Option<String>)>, AppError> {
    let defs = list_definitions(conn)?;
    let mut out = Vec::with_capacity(defs.len());
    for def in defs {
        let last: Option<String> = conn
            .query_row(
                "SELECT MAX(started_at) FROM agent_runs WHERE agent_id = ?1",
                [&def.id],
                |row| row.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten();
        out.push((def, last));
    }
    Ok(out)
}

/// [`AgentMemory`] backed by the `agent_findings` / `agent_runs` tables.
pub struct SqliteAgentMemory<'a> {
    conn: &'a Connection,
}

impl<'a> SqliteAgentMemory<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
}

impl AgentMemory for SqliteAgentMemory<'_> {
    fn append(&mut self, finding: AgentFinding) {
        let rsids_json = serde_json::to_string(&finding.rsids).unwrap_or_else(|_| "[]".to_string());
        // Best-effort: a failed finding insert must not abort the whole run.
        let _ = self.conn.execute(
            "INSERT OR REPLACE INTO agent_findings
                (id, agent_id, run_id, kind, title, detail, rsids_json, created_at, seen)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                finding.id,
                finding.agent_id,
                finding.run_id,
                kind_str(finding.kind),
                finding.title,
                finding.detail,
                rsids_json,
                finding.created_at,
                finding.seen as i64,
            ],
        );
    }

    fn findings(&self, agent_id: &str) -> Vec<AgentFinding> {
        let mut stmt = match self.conn.prepare(
            "SELECT id, agent_id, run_id, kind, title, detail, rsids_json, created_at, seen
             FROM agent_findings WHERE agent_id = ?1 ORDER BY created_at DESC, id DESC",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map([agent_id], |row| {
            let kind_s: String = row.get(3)?;
            let rsids_json: String = row.get(6)?;
            let seen_i: i64 = row.get(8)?;
            Ok(AgentFinding {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                run_id: row.get(2)?,
                kind: kind_from_str(&kind_s),
                title: row.get(4)?,
                detail: row.get(5)?,
                rsids: serde_json::from_str(&rsids_json).unwrap_or_default(),
                created_at: row.get(7)?,
                seen: seen_i != 0,
            })
        });
        match rows {
            Ok(mapped) => mapped.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        }
    }

    fn last_run_at(&self, agent_id: &str) -> Option<String> {
        self.conn
            .query_row(
                "SELECT MAX(started_at) FROM agent_runs WHERE agent_id = ?1",
                [agent_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten()
    }

    fn record_run(&mut self, _agent_id: &str, _at: &str) {
        // The full run row is persisted by `insert_run` in the command layer;
        // `last_run_at` reads it back from `agent_runs`. Nothing to do here.
    }

    fn mark_seen(&mut self, finding_id: &str) -> bool {
        self.conn
            .execute(
                "UPDATE agent_findings SET seen = 1 WHERE id = ?1",
                [finding_id],
            )
            .map(|n| n > 0)
            .unwrap_or(false)
    }
}
