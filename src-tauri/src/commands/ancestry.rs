use tauri::State;

use crate::analysis::ancestry::{self, AncestryResult};
use crate::db::Database;
use crate::error::AppError;

#[tauri::command]
pub fn get_ancestry_analysis(
    genome_id: i64,
    db: State<'_, Database>,
) -> Result<AncestryResult, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;
    ancestry::analyze_ancestry(&conn, genome_id)
}
