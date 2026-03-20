use tauri::State;

use crate::analysis::carrier::{self, CarrierResult};
use crate::db::Database;
use crate::error::AppError;

#[tauri::command]
pub fn get_carrier_status(
    genome_id: i64,
    db: State<'_, Database>,
) -> Result<Vec<CarrierResult>, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;
    carrier::analyze_carrier_status(&conn, genome_id)
}
