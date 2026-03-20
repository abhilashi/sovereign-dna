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
        AppError::Database(format!("Failed to open database at {}: {}", db_path.display(), e))
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
