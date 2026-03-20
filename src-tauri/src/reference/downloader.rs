use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use std::io::Read;

use crate::error::AppError;

/// Progress callback type for download operations.
pub type ProgressCallback = Box<dyn Fn(f64, &str) + Send>;

/// Download and decompress the ClinVar variant_summary.txt.gz file.
///
/// Saves the decompressed file to the app data directory.
/// Reports progress via the provided callback.
pub async fn download_clinvar(
    app_data_dir: &Path,
    progress_cb: Option<ProgressCallback>,
) -> Result<PathBuf, AppError> {
    let url =
        "https://ftp.ncbi.nlm.nih.gov/pub/clinvar/tab_delimited/variant_summary.txt.gz";

    let output_path = app_data_dir.join("clinvar_variant_summary.txt");

    // If already downloaded, return existing path
    if output_path.exists() {
        if let Some(ref cb) = progress_cb {
            cb(1.0, "ClinVar data already downloaded");
        }
        return Ok(output_path);
    }

    if let Some(ref cb) = progress_cb {
        cb(0.0, "Starting ClinVar download...");
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| AppError::Network(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::Network(format!("Failed to download ClinVar data: {}", e)))?;

    if !response.status().is_success() {
        return Err(AppError::Network(format!(
            "ClinVar download failed with status: {}",
            response.status()
        )));
    }

    let total_size = response.content_length().unwrap_or(0);

    if let Some(ref cb) = progress_cb {
        cb(0.1, "Downloading ClinVar data...");
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::Network(format!("Failed to read response body: {}", e)))?;

    if let Some(ref cb) = progress_cb {
        let size_mb = bytes.len() as f64 / 1_048_576.0;
        cb(0.5, &format!("Downloaded {:.1} MB, decompressing...", size_mb));
    }

    // Decompress gzip
    let mut decoder = GzDecoder::new(bytes.as_ref());
    let mut decompressed = String::new();
    decoder
        .read_to_string(&mut decompressed)
        .map_err(|e| AppError::Io(format!("Failed to decompress ClinVar data: {}", e)))?;

    if let Some(ref cb) = progress_cb {
        cb(0.9, "Saving decompressed data...");
    }

    // Write decompressed file
    std::fs::create_dir_all(app_data_dir)?;
    std::fs::write(&output_path, &decompressed)?;

    if let Some(ref cb) = progress_cb {
        let _ = total_size; // suppress unused warning
        cb(1.0, "ClinVar download complete");
    }

    Ok(output_path)
}

/// Download the GWAS Catalog full download TSV file.
///
/// The file is a plain TSV (not gzipped).
/// Saves the file to the app data directory and returns the content as a String.
pub async fn download_gwas_catalog(
    app_data_dir: &Path,
) -> Result<String, AppError> {
    let url = "https://www.ebi.ac.uk/gwas/api/search/downloads/full";

    let output_path = app_data_dir.join("gwas_catalog_full.tsv");

    // If already downloaded, return existing content
    if output_path.exists() {
        let content = std::fs::read_to_string(&output_path)
            .map_err(|e| AppError::Io(format!("Failed to read cached GWAS file: {}", e)))?;
        return Ok(content);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| AppError::Network(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::Network(format!("Failed to download GWAS Catalog: {}", e)))?;

    if !response.status().is_success() {
        return Err(AppError::Network(format!(
            "GWAS Catalog download failed with status: {}",
            response.status()
        )));
    }

    let content = response
        .text()
        .await
        .map_err(|e| AppError::Network(format!("Failed to read GWAS response body: {}", e)))?;

    // Cache to disk
    std::fs::create_dir_all(app_data_dir)?;
    std::fs::write(&output_path, &content)?;

    log::info!(
        "Downloaded GWAS Catalog: {} bytes",
        content.len()
    );

    Ok(content)
}

/// Download PharmGKB clinical annotations (placeholder URL).
pub async fn download_pharmgkb(
    app_data_dir: &Path,
    progress_cb: Option<ProgressCallback>,
) -> Result<PathBuf, AppError> {
    let output_path = app_data_dir.join("pharmgkb_clinical_annotations.tsv");

    if output_path.exists() {
        if let Some(ref cb) = progress_cb {
            cb(1.0, "PharmGKB data already downloaded");
        }
        return Ok(output_path);
    }

    if let Some(ref cb) = progress_cb {
        cb(0.0, "Starting PharmGKB download...");
    }

    // PharmGKB requires API key/license, so we provide a placeholder
    // In production, users would download the file manually or provide credentials
    let url = "https://api.pharmgkb.org/v1/download/file/data/clinicalAnnotations.zip";

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| AppError::Network(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| {
            AppError::Network(format!(
                "Failed to download PharmGKB data: {}. You may need to download manually from https://www.pharmgkb.org/downloads",
                e
            ))
        })?;

    if !response.status().is_success() {
        return Err(AppError::Network(format!(
            "PharmGKB download failed with status: {}. Manual download may be required.",
            response.status()
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::Network(format!("Failed to read response body: {}", e)))?;

    if let Some(ref cb) = progress_cb {
        cb(0.8, "Saving PharmGKB data...");
    }

    std::fs::create_dir_all(app_data_dir)?;
    std::fs::write(&output_path, &bytes)?;

    if let Some(ref cb) = progress_cb {
        cb(1.0, "PharmGKB download complete");
    }

    Ok(output_path)
}
