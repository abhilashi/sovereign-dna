pub mod queries;

use std::sync::Mutex;

use rusqlite::Connection;

use crate::error::AppError;

/// Thread-safe wrapper around a SQLite connection for use as Tauri managed state.
pub struct Database(pub Mutex<Connection>);

const MIGRATION_001: &str = include_str!("../../migrations/001_core_schema.sql");
const MIGRATION_002: &str = include_str!("../../migrations/002_annotations.sql");
const MIGRATION_003: &str = include_str!("../../migrations/003_reference_databases.sql");
const MIGRATION_004: &str = include_str!("../../migrations/004_research_digest.sql");
const MIGRATION_005: &str = include_str!("../../migrations/005_workbench.sql");
// Phase 1 and Phase 3 were developed as independent stacks from main and both
// originally started numbering at 006. The integrated train keeps Phase 1 at
// 006–007 and renumbers the dependent agent schema to 008–010.
const MIGRATION_006_VARIANT_ALLELES: &str =
    include_str!("../../migrations/006_variant_alleles.sql");
const MIGRATION_007_PROVENANCE: &str = include_str!("../../migrations/007_provenance.sql");
const MIGRATION_008_AGENTS: &str = include_str!("../../migrations/008_agents.sql");
const MIGRATION_009_AGENT_PRIVACY_LEDGER: &str =
    include_str!("../../migrations/009_agent_privacy_ledger.sql");
const MIGRATION_010_AGENT_EVENTS: &str = include_str!("../../migrations/010_agent_events.sql");

/// Initialize the SQLite database in the given app data directory.
/// Enables WAL mode and foreign keys, then runs all migrations.
pub fn initialize_database(app_data_dir: &std::path::Path) -> Result<Database, AppError> {
    std::fs::create_dir_all(app_data_dir).map_err(|e| {
        AppError::Io(format!(
            "Failed to create app data directory {}: {}",
            app_data_dir.display(),
            e
        ))
    })?;

    let db_path = app_data_dir.join("genome_studio.db");
    let conn = Connection::open(&db_path).map_err(|e| {
        AppError::Database(format!(
            "Failed to open database at {}: {}",
            db_path.display(),
            e
        ))
    })?;

    // Enable WAL mode for better concurrent read performance
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;

    // Enable foreign key enforcement
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;

    // Increase cache size for large SNP imports
    conn.execute_batch("PRAGMA cache_size=-64000;")?;

    // Run migrations
    run_migrations(&conn)?;

    Ok(Database(Mutex::new(conn)))
}

fn run_migrations(conn: &Connection) -> Result<(), AppError> {
    // Create a migrations tracking table
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL
        );",
    )?;

    let migrations: &[(&str, &str)] = &[
        ("001_core_schema", MIGRATION_001),
        ("002_annotations", MIGRATION_002),
        ("003_reference_databases", MIGRATION_003),
        ("004_research_digest", MIGRATION_004),
        ("005_workbench", MIGRATION_005),
        ("006_variant_alleles", MIGRATION_006_VARIANT_ALLELES),
        ("007_provenance", MIGRATION_007_PROVENANCE),
        ("008_agents", MIGRATION_008_AGENTS),
        (
            "009_agent_privacy_ledger",
            MIGRATION_009_AGENT_PRIVACY_LEDGER,
        ),
        ("010_agent_events", MIGRATION_010_AGENT_EVENTS),
    ];

    for (name, sql) in migrations {
        let already_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = ?1",
                [name],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !already_applied {
            conn.execute_batch(sql)?;
            conn.execute(
                "INSERT INTO _migrations (name, applied_at) VALUES (?1, datetime('now'))",
                [name],
            )?;
            log::info!("Applied migration: {}", name);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_columns(conn: &Connection, table: &str) -> Vec<String> {
        conn.prepare(&format!("PRAGMA table_info({table})"))
            .expect("prepare schema query")
            .query_map([], |row| row.get(1))
            .expect("query schema")
            .collect::<Result<_, _>>()
            .expect("collect columns")
    }

    #[test]
    fn combined_phase1_and_agent_migrations_apply_and_are_idempotent() {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch("PRAGMA foreign_keys=ON;")
            .expect("enable foreign keys");

        run_migrations(&conn).expect("apply combined migration train");
        // A second launch must not replay ALTER TABLE or duplicate schema work.
        run_migrations(&conn).expect("re-run combined migration train");

        let applied: i64 = conn
            .query_row("SELECT COUNT(*) FROM _migrations", [], |row| row.get(0))
            .expect("count migrations");
        assert_eq!(applied, 10);

        for table in [
            "genomes",
            "snps",
            "agent_definitions",
            "agent_runs",
            "agent_findings",
            "agent_consents",
            "agent_ledger",
            "agent_events",
        ] {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .expect("query schema");
            assert!(exists, "missing integrated table: {table}");
        }

        let genome_columns = table_columns(&conn, "genomes");
        for column in ["source_label", "total_lines", "skipped_lines"] {
            assert!(
                genome_columns.iter().any(|name| name == column),
                "missing Phase 1 provenance column: {column}"
            );
        }

        let snp_columns: Vec<String> = conn
            .prepare("PRAGMA table_info(snps)")
            .expect("prepare snps schema query")
            .query_map([], |row| row.get(1))
            .expect("query snps schema")
            .collect::<Result<_, _>>()
            .expect("collect snps columns");
        for column in ["ref_allele", "alt_allele", "sample"] {
            assert!(
                snp_columns.iter().any(|name| name == column),
                "missing Phase 1 variant column: {column}"
            );
        }
    }
}
