use serde::Serialize;
use tauri::State;

use crate::db::Database;
use crate::error::AppError;

// --- Structs ---

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DensityBin {
    pub bin_start: i64,
    pub bin_end: i64,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromosomeLayout {
    pub chromosome: String,
    pub snp_count: i64,
    pub min_position: i64,
    pub max_position: i64,
    pub density_bins: Vec<DensityBin>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenomeLayout {
    pub chromosomes: Vec<ChromosomeLayout>,
    pub total_snps: i64,
    pub total_span: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapSnp {
    pub rsid: String,
    pub chromosome: String,
    pub position: i64,
    pub genotype: String,
    pub gene: Option<String>,
    pub clinical_significance: Option<String>,
    pub condition: Option<String>,
    pub has_health_risk: bool,
    pub has_pharma: bool,
    pub has_trait: bool,
    pub has_carrier: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromosomeDensity {
    pub chromosome: String,
    pub bins: Vec<DensityBin>,
    pub total_snps: i64,
    pub min_position: i64,
    pub max_position: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayMarker {
    pub rsid: String,
    pub chromosome: String,
    pub position: i64,
    pub layer: String,
    pub label: String,
    pub significance: String,
}

// --- Constants ---

const PHARMA_GENES: &[&str] = &["CYP2D6", "CYP2C19", "CYP2C9", "CYP3A4", "CYP1A2"];

const TRAIT_RSIDS: &[&str] = &[
    "rs12913832",
    "rs1805007",
    "rs1815739",
    "rs762551",
    "rs671",
    "rs713598",
    "rs72921001",
];

/// SQL fragment for chromosome sort ordering (1-22 as integers, X=23, Y=24).
const CHR_ORDER_EXPR: &str = "CASE chromosome
    WHEN '1' THEN 1 WHEN '2' THEN 2 WHEN '3' THEN 3 WHEN '4' THEN 4
    WHEN '5' THEN 5 WHEN '6' THEN 6 WHEN '7' THEN 7 WHEN '8' THEN 8
    WHEN '9' THEN 9 WHEN '10' THEN 10 WHEN '11' THEN 11 WHEN '12' THEN 12
    WHEN '13' THEN 13 WHEN '14' THEN 14 WHEN '15' THEN 15 WHEN '16' THEN 16
    WHEN '17' THEN 17 WHEN '18' THEN 18 WHEN '19' THEN 19 WHEN '20' THEN 20
    WHEN '21' THEN 21 WHEN '22' THEN 22 WHEN 'X' THEN 23 WHEN 'Y' THEN 24
    ELSE 25 END";

// --- Helper functions ---

/// Compute density bins for a given position range and bin count.
fn compute_density_bins(
    conn: &rusqlite::Connection,
    genome_id: i64,
    chromosome: &str,
    min_pos: i64,
    max_pos: i64,
    num_bins: i64,
) -> Result<Vec<DensityBin>, AppError> {
    let span = max_pos - min_pos;
    if span <= 0 || num_bins <= 0 {
        return Ok(vec![]);
    }

    let bin_size = (span as f64 / num_bins as f64).ceil() as i64;
    if bin_size <= 0 {
        return Ok(vec![]);
    }

    // Use a single query that groups SNPs into bins via integer division.
    // bin_index = (position - min_pos) / bin_size
    let sql = format!(
        "SELECT (position - ?3) / ?4 AS bin_idx, COUNT(*) AS cnt
         FROM snps
         WHERE genome_id = ?1 AND chromosome = ?2
           AND position >= ?3 AND position <= ?5
         GROUP BY bin_idx
         ORDER BY bin_idx"
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params![genome_id, chromosome, min_pos, bin_size, max_pos],
        |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        },
    )?;

    // Collect the query results into a map for fast lookup.
    let mut bin_counts = std::collections::HashMap::new();
    for row in rows {
        let (idx, count) = row?;
        bin_counts.insert(idx, count);
    }

    // Build the full bin vector (including empty bins).
    let effective_bins = num_bins.min(span / bin_size.max(1) + 1);
    let mut bins = Vec::with_capacity(effective_bins as usize);
    for i in 0..effective_bins {
        let bin_start = min_pos + i * bin_size;
        let bin_end = (bin_start + bin_size - 1).min(max_pos);
        let count = bin_counts.get(&i).copied().unwrap_or(0);
        bins.push(DensityBin {
            bin_start,
            bin_end,
            count,
        });
    }

    Ok(bins)
}

// --- Commands ---

/// Returns the chromosome layout info needed to render the full genome view.
#[tauri::command]
pub fn get_genome_layout(
    genome_id: i64,
    db: State<'_, Database>,
) -> Result<GenomeLayout, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;

    // Get per-chromosome summary stats, ordered 1-22, X, Y.
    let summary_sql = format!(
        "SELECT chromosome, COUNT(*) AS snp_count, MIN(position) AS min_pos, MAX(position) AS max_pos
         FROM snps
         WHERE genome_id = ?1
         GROUP BY chromosome
         ORDER BY {}",
        CHR_ORDER_EXPR
    );

    let mut stmt = conn.prepare(&summary_sql)?;
    let chr_rows: Vec<(String, i64, i64, i64)> = stmt
        .query_map(rusqlite::params![genome_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut chromosomes = Vec::with_capacity(chr_rows.len());
    let mut total_snps: i64 = 0;
    let mut total_span: i64 = 0;

    for (chromosome, snp_count, min_position, max_position) in &chr_rows {
        let density_bins = compute_density_bins(
            &conn,
            genome_id,
            chromosome,
            *min_position,
            *max_position,
            50,
        )?;

        total_snps += snp_count;
        total_span += max_position - min_position;

        chromosomes.push(ChromosomeLayout {
            chromosome: chromosome.clone(),
            snp_count: *snp_count,
            min_position: *min_position,
            max_position: *max_position,
            density_bins,
        });
    }

    Ok(GenomeLayout {
        chromosomes,
        total_snps,
        total_span,
    })
}

/// Returns SNPs for a specific genomic region (used when the user zooms in).
#[tauri::command]
pub fn get_region_snps(
    genome_id: i64,
    chromosome: String,
    start: i64,
    end: i64,
    db: State<'_, Database>,
) -> Result<Vec<MapSnp>, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;

    const MAX_RESULTS: i64 = 5000;

    // First, count how many SNPs are in the region to decide whether to sample.
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM snps
         WHERE genome_id = ?1 AND chromosome = ?2 AND position >= ?3 AND position <= ?4",
        rusqlite::params![genome_id, chromosome, start, end],
        |row| row.get(0),
    )?;

    // Build the query. If the region has more than MAX_RESULTS SNPs, sample
    // by selecting every Nth row using a subquery with ROW_NUMBER.
    let sql = if total > MAX_RESULTS {
        let nth = (total as f64 / MAX_RESULTS as f64).ceil() as i64;
        format!(
            "SELECT rsid, chromosome, position, genotype, gene,
                    clinical_significance, condition
             FROM (
                 SELECT s.rsid, s.chromosome, s.position, s.genotype,
                        a.gene, a.clinical_significance, a.condition,
                        ROW_NUMBER() OVER (ORDER BY s.position) AS rn
                 FROM snps s
                 LEFT JOIN annotations a ON s.rsid = a.rsid
                 WHERE s.genome_id = ?1 AND s.chromosome = ?2
                   AND s.position >= ?3 AND s.position <= ?4
             ) sub
             WHERE sub.rn % {} = 1
             LIMIT {}",
            nth, MAX_RESULTS
        )
    } else {
        "SELECT s.rsid, s.chromosome, s.position, s.genotype,
                a.gene, a.clinical_significance, a.condition
         FROM snps s
         LEFT JOIN annotations a ON s.rsid = a.rsid
         WHERE s.genome_id = ?1 AND s.chromosome = ?2
           AND s.position >= ?3 AND s.position <= ?4
         ORDER BY s.position
         LIMIT 5000"
            .to_string()
    };

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params![genome_id, chromosome, start, end],
        |row| {
            let rsid: String = row.get(0)?;
            let gene: Option<String> = row.get(4)?;
            let clinical_significance: Option<String> = row.get(5)?;
            let condition: Option<String> = row.get(6)?;

            // Compute overlay flags from annotation data.
            let clin_lower = clinical_significance
                .as_deref()
                .unwrap_or("")
                .to_lowercase();

            let has_health_risk = !clin_lower.is_empty()
                && (clin_lower.contains("pathogenic") || clin_lower.contains("risk"));

            let has_pharma = gene
                .as_deref()
                .map(|g| PHARMA_GENES.iter().any(|pg| g.contains(pg)))
                .unwrap_or(false);

            let has_trait = TRAIT_RSIDS.contains(&rsid.as_str());

            let has_carrier = clin_lower.contains("pathogenic") && condition.is_some();

            Ok(MapSnp {
                rsid,
                chromosome: row.get(1)?,
                position: row.get(2)?,
                genotype: row.get(3)?,
                gene,
                clinical_significance: row.get(5)?,
                condition,
                has_health_risk,
                has_pharma,
                has_trait,
                has_carrier,
            })
        },
    )?;

    let results: Vec<MapSnp> = rows.collect::<Result<Vec<_>, _>>()?;
    Ok(results)
}

/// Returns density data for a single chromosome at a specified resolution.
#[tauri::command]
pub fn get_chromosome_density(
    genome_id: i64,
    chromosome: String,
    num_bins: i64,
    db: State<'_, Database>,
) -> Result<ChromosomeDensity, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;

    let (total_snps, min_position, max_position): (i64, i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(MIN(position), 0), COALESCE(MAX(position), 0)
         FROM snps
         WHERE genome_id = ?1 AND chromosome = ?2",
        rusqlite::params![genome_id, chromosome],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;

    let bins = if total_snps > 0 {
        compute_density_bins(&conn, genome_id, &chromosome, min_position, max_position, num_bins)?
    } else {
        vec![]
    };

    Ok(ChromosomeDensity {
        chromosome,
        bins,
        total_snps,
        min_position,
        max_position,
    })
}

/// Returns all SNPs flagged by any analysis module, for rendering as overlay markers.
/// A single SNP can produce multiple markers if it matches multiple overlay categories.
#[tauri::command]
pub fn get_analysis_overlay(
    genome_id: i64,
    db: State<'_, Database>,
) -> Result<Vec<OverlayMarker>, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;

    let mut markers: Vec<OverlayMarker> = Vec::new();

    // --- Health risk markers ---
    // SNPs where clinical_significance contains "pathogenic" or "risk".
    {
        let mut stmt = conn.prepare(
            "SELECT s.rsid, s.chromosome, s.position,
                    a.gene, a.clinical_significance, a.condition
             FROM snps s
             INNER JOIN annotations a ON s.rsid = a.rsid
             WHERE s.genome_id = ?1
               AND (LOWER(a.clinical_significance) LIKE '%pathogenic%'
                    OR LOWER(a.clinical_significance) LIKE '%risk%')"
        )?;

        let rows = stmt.query_map(rusqlite::params![genome_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;

        for row in rows {
            let (rsid, chromosome, position, gene, clin_sig, condition) = row?;
            markers.push(OverlayMarker {
                rsid,
                chromosome,
                position,
                layer: "health".to_string(),
                label: condition
                    .or(gene)
                    .unwrap_or_else(|| "Unknown".to_string()),
                significance: clin_sig.unwrap_or_else(|| "Unknown".to_string()),
            });
        }
    }

    // --- Pharmacogenomic markers ---
    // SNPs whose annotated gene is one of the known pharmacogenes.
    {
        // Build a parameterized IN clause for pharma genes.
        let placeholders: Vec<String> = PHARMA_GENES
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 2))
            .collect();
        let in_clause = placeholders.join(", ");

        let sql = format!(
            "SELECT s.rsid, s.chromosome, s.position,
                    a.gene, a.clinical_significance
             FROM snps s
             INNER JOIN annotations a ON s.rsid = a.rsid
             WHERE s.genome_id = ?1
               AND a.gene IN ({})",
            in_clause
        );

        let mut stmt = conn.prepare(&sql)?;
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        params.push(Box::new(genome_id));
        for gene in PHARMA_GENES {
            params.push(Box::new(gene.to_string()));
        }
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;

        for row in rows {
            let (rsid, chromosome, position, gene, clin_sig) = row?;
            markers.push(OverlayMarker {
                rsid,
                chromosome,
                position,
                layer: "pharma".to_string(),
                label: gene.unwrap_or_else(|| "Unknown".to_string()),
                significance: clin_sig.unwrap_or_else(|| "Normal".to_string()),
            });
        }
    }

    // --- Trait markers ---
    // SNPs matching known trait-associated rsIDs.
    {
        let placeholders: Vec<String> = TRAIT_RSIDS
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 2))
            .collect();
        let in_clause = placeholders.join(", ");

        let sql = format!(
            "SELECT s.rsid, s.chromosome, s.position,
                    a.gene, a.condition
             FROM snps s
             LEFT JOIN annotations a ON s.rsid = a.rsid
             WHERE s.genome_id = ?1
               AND s.rsid IN ({})",
            in_clause
        );

        let mut stmt = conn.prepare(&sql)?;
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        params.push(Box::new(genome_id));
        for rsid in TRAIT_RSIDS {
            params.push(Box::new(rsid.to_string()));
        }
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;

        for row in rows {
            let (rsid, chromosome, position, gene, condition) = row?;
            let label = condition
                .or(gene)
                .unwrap_or_else(|| rsid.clone());
            markers.push(OverlayMarker {
                rsid,
                chromosome,
                position,
                layer: "trait".to_string(),
                label,
                significance: "Trait-associated".to_string(),
            });
        }
    }

    // --- Carrier status markers ---
    // SNPs with pathogenic clinical significance AND a known condition.
    {
        let mut stmt = conn.prepare(
            "SELECT s.rsid, s.chromosome, s.position,
                    a.gene, a.clinical_significance, a.condition
             FROM snps s
             INNER JOIN annotations a ON s.rsid = a.rsid
             WHERE s.genome_id = ?1
               AND LOWER(a.clinical_significance) LIKE '%pathogenic%'
               AND a.condition IS NOT NULL AND a.condition != ''"
        )?;

        let rows = stmt.query_map(rusqlite::params![genome_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;

        for row in rows {
            let (rsid, chromosome, position, _gene, clin_sig, condition) = row?;
            markers.push(OverlayMarker {
                rsid,
                chromosome,
                position,
                layer: "carrier".to_string(),
                label: condition.unwrap_or_else(|| "Unknown condition".to_string()),
                significance: clin_sig.unwrap_or_else(|| "Pathogenic".to_string()),
            });
        }
    }

    Ok(markers)
}
