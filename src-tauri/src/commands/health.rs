use tauri::State;

use crate::analysis::health_risk::{self, HealthRiskResult};
use crate::db::Database;
use crate::error::AppError;

#[tauri::command]
pub fn get_health_risks(
    genome_id: i64,
    db: State<'_, Database>,
) -> Result<Vec<HealthRiskResult>, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;
    health_risk::analyze_health_risks(&conn, genome_id)
}

#[tauri::command]
pub fn get_health_risk_detail(
    genome_id: i64,
    condition: String,
    db: State<'_, Database>,
) -> Result<HealthRiskResult, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;

    let all_risks = health_risk::analyze_health_risks(&conn, genome_id)?;

    all_risks
        .into_iter()
        .find(|r| r.condition.eq_ignore_ascii_case(&condition))
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "No health risk analysis found for condition: {}",
                condition
            ))
        })
}
