use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::queries::{self, AnnotatedSnp, SnpRow};
use crate::db::Database;
use crate::error::AppError;

/// Paginated SNP results.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnpPage {
    pub rows: Vec<SnpRow>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

/// Detailed SNP information with annotations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnpDetail {
    pub snp: SnpRow,
    pub annotations: Vec<AnnotatedSnp>,
}

#[tauri::command]
pub fn get_snps(
    genome_id: i64,
    offset: i64,
    limit: i64,
    search: Option<String>,
    chromosome: Option<String>,
    db: State<'_, Database>,
) -> Result<SnpPage, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;

    let (rows, total) = queries::get_snps_paginated(
        &conn,
        genome_id,
        offset,
        limit,
        search.as_deref(),
        chromosome.as_deref(),
    )?;

    Ok(SnpPage {
        rows,
        total,
        offset,
        limit,
    })
}

#[tauri::command]
pub fn get_snp_detail(
    genome_id: i64,
    rsid: String,
    db: State<'_, Database>,
) -> Result<SnpDetail, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;

    let snp = queries::get_snp_by_rsid(&conn, genome_id, &rsid)?;

    // Get annotations for this specific rsid
    let mut stmt = conn.prepare(
        "SELECT s.rsid, s.chromosome, s.position, s.genotype,
                a.gene, a.clinical_significance, a.condition,
                a.review_status, a.allele_frequency, a.source
         FROM snps s
         LEFT JOIN annotations a ON s.rsid = a.rsid
         WHERE s.genome_id = ?1 AND s.rsid = ?2",
    )?;

    let annotations: Vec<AnnotatedSnp> = stmt
        .query_map(rusqlite::params![genome_id, rsid], |row| {
            Ok(AnnotatedSnp {
                rsid: row.get(0)?,
                chromosome: row.get(1)?,
                position: row.get(2)?,
                genotype: row.get(3)?,
                gene: row.get(4)?,
                clinical_significance: row.get(5)?,
                condition: row.get(6)?,
                review_status: row.get(7)?,
                allele_frequency: row.get(8)?,
                source: row.get(9)?,
            })
        })?
        .filter_map(|r| r.ok())
        // Filter out entries where all annotation fields are null (no real annotation)
        .filter(|a| {
            a.gene.is_some()
                || a.clinical_significance.is_some()
                || a.condition.is_some()
        })
        .collect();

    Ok(SnpDetail { snp, annotations })
}

#[tauri::command]
pub fn export_snps(
    genome_id: i64,
    format: String,
    filter: Option<String>,
    db: State<'_, Database>,
) -> Result<String, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;

    // Determine which SNPs to export
    let query = if let Some(ref filter_type) = filter {
        match filter_type.as_str() {
            "annotated" => {
                "SELECT s.rsid, s.chromosome, s.position, s.genotype,
                        a.gene, a.clinical_significance, a.condition
                 FROM snps s
                 INNER JOIN annotations a ON s.rsid = a.rsid
                 WHERE s.genome_id = ?1
                 ORDER BY s.chromosome, s.position"
                    .to_string()
            }
            "pathogenic" => {
                "SELECT s.rsid, s.chromosome, s.position, s.genotype,
                        a.gene, a.clinical_significance, a.condition
                 FROM snps s
                 INNER JOIN annotations a ON s.rsid = a.rsid
                 WHERE s.genome_id = ?1
                 AND (a.clinical_significance LIKE '%pathogenic%')
                 ORDER BY s.chromosome, s.position"
                    .to_string()
            }
            _ => {
                "SELECT rsid, chromosome, position, genotype, '', '', ''
                 FROM snps WHERE genome_id = ?1
                 ORDER BY chromosome, position"
                    .to_string()
            }
        }
    } else {
        "SELECT rsid, chromosome, position, genotype, '', '', ''
         FROM snps WHERE genome_id = ?1
         ORDER BY chromosome, position"
            .to_string()
    };

    let mut stmt = conn.prepare(&query)?;
    let rows: Vec<(String, String, i64, String, String, String, String)> = stmt
        .query_map([genome_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4).unwrap_or_default(),
                row.get::<_, String>(5).unwrap_or_default(),
                row.get::<_, String>(6).unwrap_or_default(),
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Generate output in the requested format
    let _genome = queries::get_genome(&conn, genome_id)?;
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let extension = match format.as_str() {
        "json" => "json",
        _ => "csv",
    };

    let filename = format!(
        "genome_studio_export_{}_{}_{}.{}",
        genome_id,
        filter.as_deref().unwrap_or("all"),
        timestamp,
        extension
    );

    // Use temp directory for export
    let output_dir = std::env::temp_dir().join("genome_studio_exports");
    std::fs::create_dir_all(&output_dir)?;
    let output_path = output_dir.join(&filename);

    match format.as_str() {
        "json" => {
            #[derive(Serialize)]
            struct ExportRow {
                rsid: String,
                chromosome: String,
                position: i64,
                genotype: String,
                gene: String,
                clinical_significance: String,
                condition: String,
            }

            let export_rows: Vec<ExportRow> = rows
                .iter()
                .map(|(rsid, chr, pos, gt, gene, sig, cond)| ExportRow {
                    rsid: rsid.clone(),
                    chromosome: chr.clone(),
                    position: *pos,
                    genotype: gt.clone(),
                    gene: gene.clone(),
                    clinical_significance: sig.clone(),
                    condition: cond.clone(),
                })
                .collect();

            let json = serde_json::to_string_pretty(&export_rows)?;
            std::fs::write(&output_path, json)?;
        }
        _ => {
            // CSV format
            let mut wtr = csv::Writer::from_path(&output_path)
                .map_err(|e| AppError::Io(format!("Failed to create CSV writer: {}", e)))?;

            wtr.write_record(["rsid", "chromosome", "position", "genotype", "gene", "clinical_significance", "condition"])
                .map_err(|e| AppError::Io(format!("Failed to write CSV header: {}", e)))?;

            for (rsid, chr, pos, gt, gene, sig, cond) in &rows {
                wtr.write_record([rsid, chr, &pos.to_string(), gt, gene, sig, cond])
                    .map_err(|e| AppError::Io(format!("Failed to write CSV row: {}", e)))?;
            }

            wtr.flush()
                .map_err(|e| AppError::Io(format!("Failed to flush CSV: {}", e)))?;
        }
    }

    Ok(output_path.to_string_lossy().to_string())
}
