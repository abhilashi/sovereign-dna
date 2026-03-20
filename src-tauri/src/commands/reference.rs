use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::Manager;
use tauri::State;

use crate::db::queries::{self, ReferenceStatus};
use crate::db::Database;
use crate::error::AppError;
use crate::reference::manager::{self, ReferenceLoadResult};

/// Progress update sent to the frontend during reference database download.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceProgress {
    pub source: String,
    pub phase: String,
    pub progress: f64,
    pub message: String,
}

#[tauri::command]
pub async fn download_reference_database(
    source: String,
    genome_id: Option<i64>,
    app_handle: tauri::AppHandle,
    db: State<'_, Database>,
    channel: Channel<ReferenceProgress>,
) -> Result<ReferenceLoadResult, AppError> {
    // Get app data directory
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Io(format!("Failed to get app data directory: {}", e)))?;

    // Step 1: Lock DB briefly to get user rsIDs, then release
    let user_rsids: HashSet<String> = if let Some(gid) = genome_id {
        let conn = db.0.lock().map_err(|e| {
            AppError::Database(format!("Failed to acquire database lock: {}", e))
        })?;
        queries::get_all_user_rsids(&conn, gid)?
        // MutexGuard dropped here
    } else {
        HashSet::new()
    };

    // Also lock briefly to set initial status
    {
        let conn = db.0.lock().map_err(|e| {
            AppError::Database(format!("Failed to acquire database lock: {}", e))
        })?;
        queries::upsert_reference_status(&conn, &source, "downloading", 0, 0, None, None)?;
        // MutexGuard dropped here
    }

    let source_for_cb = source.clone();
    let progress_cb = move |progress: f64, message: &str| {
        let phase = if progress < 0.5 {
            "downloading"
        } else if progress < 0.9 {
            "parsing"
        } else if progress < 1.0 {
            "inserting"
        } else {
            "complete"
        };

        let _ = channel.send(ReferenceProgress {
            source: source_for_cb.clone(),
            phase: phase.to_string(),
            progress,
            message: message.to_string(),
        });
    };

    // Step 2: Execute async download phase (no DB lock held)
    // Step 3: Lock DB for sync parse+insert phase
    let result = match source.as_str() {
        "clinvar" => {
            // Async: download
            let clinvar_path = manager::download_clinvar_data(&app_data_dir, &progress_cb).await?;
            // Sync: lock DB, parse, insert
            let conn = db.0.lock().map_err(|e| {
                AppError::Database(format!("Failed to acquire database lock: {}", e))
            })?;
            manager::parse_and_store_clinvar(&conn, &clinvar_path, &user_rsids, &progress_cb)?
        }
        "gwas_catalog" => {
            // Async: download
            let content = manager::download_gwas_data(&app_data_dir, &progress_cb).await?;
            // Sync: lock DB, parse, insert
            let conn = db.0.lock().map_err(|e| {
                AppError::Database(format!("Failed to acquire database lock: {}", e))
            })?;
            manager::parse_and_store_gwas(&conn, &content, &user_rsids, &progress_cb)?
        }
        "snpedia" => {
            // Async: fetch from API
            let entries = manager::fetch_snpedia_data(&user_rsids, &progress_cb).await?;
            // Sync: lock DB, insert
            let conn = db.0.lock().map_err(|e| {
                AppError::Database(format!("Failed to acquire database lock: {}", e))
            })?;
            manager::store_snpedia(&conn, &entries, &progress_cb)?
        }
        _ => {
            return Err(AppError::Parse(format!(
                "Unknown reference source: {}. Valid sources: clinvar, gwas_catalog, snpedia",
                source
            )));
        }
    };

    Ok(result)
}

#[tauri::command]
pub fn get_reference_databases_status(
    db: State<'_, Database>,
) -> Result<Vec<ReferenceStatus>, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;
    queries::get_all_reference_status(&conn)
}

#[tauri::command]
pub fn delete_reference_database(
    source: String,
    db: State<'_, Database>,
) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;
    queries::clear_reference_data(&conn, &source)
}
