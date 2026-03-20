use tauri::State;

use crate::db::queries;
use crate::db::Database;
use crate::error::AppError;
use crate::report::pdf;

#[tauri::command]
pub fn generate_report(
    genome_id: i64,
    db: State<'_, Database>,
) -> Result<String, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;

    // Verify genome exists
    let genome = queries::get_genome(&conn, genome_id)?;

    // Generate report to temp directory
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!(
        "genome_studio_report_{}_{}.txt",
        genome.filename.replace('.', "_"),
        timestamp
    );

    let output_dir = std::env::temp_dir().join("genome_studio_reports");
    std::fs::create_dir_all(&output_dir)?;
    let output_path = output_dir.join(&filename);

    pdf::generate_report(&conn, genome_id, &output_path)?;

    Ok(output_path.to_string_lossy().to_string())
}
