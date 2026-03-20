use tauri::State;

use crate::analysis::traits::{self, TraitResult};
use crate::db::Database;
use crate::error::AppError;

#[tauri::command]
pub fn get_trait_predictions(
    genome_id: i64,
    db: State<'_, Database>,
) -> Result<Vec<TraitResult>, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;
    traits::analyze_traits(&conn, genome_id)
}
