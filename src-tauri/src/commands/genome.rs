use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::queries::{self, ChromosomeCount, Genome};
use crate::db::Database;
use crate::error::AppError;

/// Summary of a genome including derived statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenomeSummary {
    pub genome: Genome,
    pub chromosome_counts: Vec<ChromosomeCount>,
    pub heterozygosity_rate: f64,
    pub missing_data_percent: f64,
    pub total_snps: i64,
}

#[tauri::command]
pub fn list_genomes(db: State<'_, Database>) -> Result<Vec<Genome>, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;
    queries::get_genomes(&conn)
}

#[tauri::command]
pub fn get_genome_summary(
    genome_id: i64,
    db: State<'_, Database>,
) -> Result<GenomeSummary, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;

    let genome = queries::get_genome(&conn, genome_id)?;
    let chromosome_counts = queries::get_snp_count_by_chromosome(&conn, genome_id)?;

    // Calculate heterozygosity rate (proportion of heterozygous SNPs)
    let het_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM snps WHERE genome_id = ?1
             AND LENGTH(genotype) = 2
             AND SUBSTR(genotype, 1, 1) != SUBSTR(genotype, 2, 1)",
            [genome_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let total_diploid: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM snps WHERE genome_id = ?1 AND LENGTH(genotype) = 2",
            [genome_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let heterozygosity_rate = if total_diploid > 0 {
        (het_count as f64 / total_diploid as f64 * 1000.0).round() / 1000.0
    } else {
        0.0
    };

    // Calculate missing/no-call data percentage
    let nocall_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM snps WHERE genome_id = ?1
             AND (genotype = '--' OR genotype = '00' OR genotype = '' OR genotype = '..')",
            [genome_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let total_snps = genome.snp_count;
    let missing_data_percent = if total_snps > 0 {
        (nocall_count as f64 / total_snps as f64 * 10000.0).round() / 100.0
    } else {
        0.0
    };

    Ok(GenomeSummary {
        genome,
        chromosome_counts,
        heterozygosity_rate,
        missing_data_percent,
        total_snps,
    })
}

#[tauri::command]
pub fn delete_genome(genome_id: i64, db: State<'_, Database>) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;
    queries::delete_genome(&conn, genome_id)
}
