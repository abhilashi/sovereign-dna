use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

use crate::db::queries::{self, SnpRow};
use crate::db::Database;
use crate::error::AppError;
use crate::parser;

/// Progress update sent to the frontend during import.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub phase: String,
    pub progress: f64,
    pub message: String,
}

/// Result of a genome import operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub genome_id: i64,
    pub snp_count: usize,
    pub format: String,
    pub build: Option<String>,
    pub quality_summary: QualitySummary,
}

/// Quality metrics for the imported genome data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualitySummary {
    pub total_lines: usize,
    pub skipped_lines: usize,
    pub valid_snps: usize,
    pub skip_rate: f64,
}

#[tauri::command]
pub async fn import_genome(
    file_path: String,
    db: State<'_, Database>,
    channel: Channel<ImportProgress>,
) -> Result<ImportResult, AppError> {
    // Phase 1: Read file
    let _ = channel.send(ImportProgress {
        phase: "reading".to_string(),
        progress: 0.0,
        message: "Reading file...".to_string(),
    });

    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| AppError::Io(format!("Failed to read file {}: {}", file_path, e)))?;

    let _ = channel.send(ImportProgress {
        phase: "reading".to_string(),
        progress: 0.2,
        message: format!("Read {} bytes", content.len()),
    });

    // Phase 2: Detect format
    let format = parser::detect_format(&content);
    if format == "unknown" {
        return Err(AppError::Parse(
            "Unable to detect file format. Supported formats: 23andMe, AncestryDNA, VCF."
                .to_string(),
        ));
    }

    let _ = channel.send(ImportProgress {
        phase: "parsing".to_string(),
        progress: 0.3,
        message: format!("Detected format: {}", format),
    });

    // Phase 3: Parse
    let parse_result = match format.as_str() {
        "23andme_v5" | "23andme_v3" => parser::twentythree::parse_23andme(&content, &format)?,
        "ancestry" => parser::ancestry::parse_ancestry(&content)?,
        "vcf" => parser::vcf::parse_vcf(&content)?,
        _ => {
            return Err(AppError::Parse(format!(
                "Unsupported file format: {}",
                format
            )));
        }
    };

    let _ = channel.send(ImportProgress {
        phase: "parsing".to_string(),
        progress: 0.5,
        message: format!("Parsed {} SNPs", parse_result.snps.len()),
    });

    let snp_count = parse_result.snps.len();

    // Extract filename from path
    let filename = std::path::Path::new(&file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Phase 4: Insert into database
    let _ = channel.send(ImportProgress {
        phase: "storing".to_string(),
        progress: 0.6,
        message: "Inserting genome record...".to_string(),
    });

    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;

    let genome_id = queries::insert_genome(
        &conn,
        &filename,
        &format,
        snp_count as i64,
        parse_result.build.as_deref(),
    )?;

    // Convert parsed SNPs to SnpRows for batch insert
    let snp_rows: Vec<SnpRow> = parse_result
        .snps
        .iter()
        .map(|s| SnpRow {
            id: None,
            genome_id,
            rsid: s.rsid.clone(),
            chromosome: s.chromosome.clone(),
            position: s.position,
            genotype: s.genotype.clone(),
        })
        .collect();

    // Insert in batches for progress reporting
    let batch_size = 50_000;
    let total_batches = (snp_rows.len() + batch_size - 1) / batch_size;

    for (i, chunk) in snp_rows.chunks(batch_size).enumerate() {
        let progress = 0.6 + (0.35 * (i as f64 / total_batches as f64));
        let _ = channel.send(ImportProgress {
            phase: "storing".to_string(),
            progress,
            message: format!(
                "Inserting SNPs batch {}/{}...",
                i + 1,
                total_batches
            ),
        });

        queries::insert_snps_batch(&conn, genome_id, chunk)?;
    }

    let _ = channel.send(ImportProgress {
        phase: "complete".to_string(),
        progress: 1.0,
        message: format!(
            "Import complete: {} SNPs from {} format",
            snp_count, format
        ),
    });

    let skip_rate = if parse_result.total_lines > 0 {
        (parse_result.skipped_lines as f64 / parse_result.total_lines as f64 * 100.0).round()
            / 100.0
    } else {
        0.0
    };

    Ok(ImportResult {
        genome_id,
        snp_count,
        format,
        build: parse_result.build,
        quality_summary: QualitySummary {
            total_lines: parse_result.total_lines,
            skipped_lines: parse_result.skipped_lines,
            valid_snps: snp_count,
            skip_rate,
        },
    })
}
