use tauri::State;

use crate::analysis::pharmacogenomics::{self, PharmaResult};
use crate::db::Database;
use crate::error::AppError;

#[tauri::command]
pub fn get_pharmacogenomics(
    genome_id: i64,
    db: State<'_, Database>,
) -> Result<Vec<PharmaResult>, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;
    pharmacogenomics::analyze_pharmacogenomics(&conn, genome_id)
}
