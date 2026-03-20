use std::collections::HashSet;
use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::queries;
use crate::error::AppError;
use crate::reference::clinvar;
use crate::reference::downloader;
use crate::reference::gwas;
use crate::reference::snpedia;

/// Result of loading a reference database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceLoadResult {
    pub source: String,
    pub record_count: i64,
    pub duration_secs: f64,
}

/// Download ClinVar data (async phase — no DB access).
/// Returns the path to the downloaded/cached file.
pub async fn download_clinvar_data(
    app_data_dir: &Path,
    progress_cb: &(dyn Fn(f64, &str) + Send + Sync),
) -> Result<std::path::PathBuf, AppError> {
    progress_cb(0.0, "Downloading ClinVar data...");
    let clinvar_path = downloader::download_clinvar(app_data_dir, None).await?;
    progress_cb(0.4, "ClinVar download complete");
    Ok(clinvar_path)
}

/// Parse and store ClinVar data (sync phase — requires DB connection).
/// Call this after `download_clinvar_data` with the DB locked.
pub fn parse_and_store_clinvar(
    conn: &Connection,
    clinvar_path: &Path,
    user_rsids: &HashSet<String>,
    progress_cb: &(dyn Fn(f64, &str) + Send + Sync),
) -> Result<ReferenceLoadResult, AppError> {
    let start = std::time::Instant::now();

    let file_size = std::fs::metadata(clinvar_path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);

    queries::upsert_reference_status(conn, "clinvar", "parsing", 0, file_size, None, None)?;

    progress_cb(0.5, "Parsing ClinVar data...");

    let content = std::fs::read_to_string(clinvar_path)
        .map_err(|e| AppError::Io(format!("Failed to read ClinVar file: {}", e)))?;

    let annotations = clinvar::parse_clinvar(&content, user_rsids)?;
    let record_count = annotations.len() as i64;

    progress_cb(0.8, &format!("Inserting {} ClinVar annotations...", record_count));

    queries::upsert_annotations(conn, &annotations)?;

    queries::upsert_reference_status(
        conn,
        "clinvar",
        "complete",
        record_count,
        file_size,
        None,
        Some("latest"),
    )?;

    let duration = start.elapsed().as_secs_f64();
    progress_cb(1.0, &format!("ClinVar complete: {} annotations in {:.1}s", record_count, duration));

    Ok(ReferenceLoadResult {
        source: "clinvar".to_string(),
        record_count,
        duration_secs: duration,
    })
}

/// Download GWAS Catalog data (async phase — no DB access).
/// Returns the downloaded content as a String.
pub async fn download_gwas_data(
    app_data_dir: &Path,
    progress_cb: &(dyn Fn(f64, &str) + Send + Sync),
) -> Result<String, AppError> {
    progress_cb(0.0, "Downloading GWAS Catalog...");
    let content = downloader::download_gwas_catalog(app_data_dir).await?;
    progress_cb(0.4, "GWAS Catalog download complete");
    Ok(content)
}

/// Parse and store GWAS Catalog data (sync phase — requires DB connection).
/// Call this after `download_gwas_data` with the DB locked.
pub fn parse_and_store_gwas(
    conn: &Connection,
    content: &str,
    user_rsids: &HashSet<String>,
    progress_cb: &(dyn Fn(f64, &str) + Send + Sync),
) -> Result<ReferenceLoadResult, AppError> {
    let start = std::time::Instant::now();
    let file_size = content.len() as i64;

    queries::upsert_reference_status(conn, "gwas_catalog", "parsing", 0, file_size, None, None)?;

    progress_cb(0.5, "Parsing GWAS Catalog...");

    let associations = gwas::parse_gwas_catalog(content, user_rsids)?;
    let record_count = associations.len() as i64;

    progress_cb(0.8, &format!("Inserting {} GWAS associations...", record_count));

    // Insert in batches
    let batch_size = 10_000;
    let total_batches = (associations.len() + batch_size - 1) / batch_size;
    for (i, chunk) in associations.chunks(batch_size).enumerate() {
        let batch_progress = 0.8 + (0.15 * ((i + 1) as f64 / total_batches.max(1) as f64));
        progress_cb(
            batch_progress.min(0.95),
            &format!("Inserting GWAS batch {}/{}...", i + 1, total_batches),
        );
        queries::insert_gwas_batch(conn, chunk)?;
    }

    queries::upsert_reference_status(
        conn,
        "gwas_catalog",
        "complete",
        record_count,
        file_size,
        None,
        Some("latest"),
    )?;

    let duration = start.elapsed().as_secs_f64();
    progress_cb(1.0, &format!("GWAS Catalog complete: {} associations in {:.1}s", record_count, duration));

    Ok(ReferenceLoadResult {
        source: "gwas_catalog".to_string(),
        record_count,
        duration_secs: duration,
    })
}

/// Fetch SNPedia data (async phase — no DB access).
/// Returns the fetched entries.
pub async fn fetch_snpedia_data(
    user_rsids: &HashSet<String>,
    progress_cb: &(dyn Fn(f64, &str) + Send + Sync),
) -> Result<Vec<queries::SnpediaEntry>, AppError> {
    let max_rsids = 2000;
    let sample: Vec<String> = user_rsids
        .iter()
        .filter(|rsid| rsid.starts_with("rs"))
        .take(max_rsids)
        .cloned()
        .collect();

    if sample.is_empty() {
        return Err(AppError::Analysis(
            "No valid rsIDs found in user genome for SNPedia lookup".to_string(),
        ));
    }

    progress_cb(0.0, &format!("Fetching SNPedia data for {} rsIDs...", sample.len()));

    let entries = snpedia::fetch_snpedia_for_rsids(&sample).await?;
    progress_cb(0.7, &format!("Fetched {} SNPedia entries", entries.len()));

    Ok(entries)
}

/// Store SNPedia data (sync phase — requires DB connection).
/// Call this after `fetch_snpedia_data` with the DB locked.
pub fn store_snpedia(
    conn: &Connection,
    entries: &[queries::SnpediaEntry],
    progress_cb: &(dyn Fn(f64, &str) + Send + Sync),
) -> Result<ReferenceLoadResult, AppError> {
    let start = std::time::Instant::now();
    let record_count = entries.len() as i64;

    progress_cb(0.8, &format!("Inserting {} SNPedia entries...", record_count));

    queries::insert_snpedia_batch(conn, entries)?;

    queries::upsert_reference_status(
        conn,
        "snpedia",
        "complete",
        record_count,
        0,
        None,
        Some("latest"),
    )?;

    let duration = start.elapsed().as_secs_f64();
    progress_cb(1.0, &format!("SNPedia complete: {} entries in {:.1}s", record_count, duration));

    Ok(ReferenceLoadResult {
        source: "snpedia".to_string(),
        record_count,
        duration_secs: duration,
    })
}
