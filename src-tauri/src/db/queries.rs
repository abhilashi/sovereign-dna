use std::collections::HashSet;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

// ── Data Structures ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Genome {
    pub id: i64,
    pub filename: String,
    pub format: String,
    pub imported_at: String,
    pub snp_count: i64,
    pub build: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnpRow {
    pub id: Option<i64>,
    pub genome_id: i64,
    pub rsid: String,
    pub chromosome: String,
    pub position: i64,
    pub genotype: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromosomeCount {
    pub chromosome: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub rsid: String,
    pub gene: Option<String>,
    pub clinical_significance: Option<String>,
    pub condition: Option<String>,
    pub review_status: Option<String>,
    pub allele_frequency: Option<f64>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotatedSnp {
    pub rsid: String,
    pub chromosome: String,
    pub position: i64,
    pub genotype: String,
    pub gene: Option<String>,
    pub clinical_significance: Option<String>,
    pub condition: Option<String>,
    pub review_status: Option<String>,
    pub allele_frequency: Option<f64>,
    pub source: Option<String>,
}

// ── Genome CRUD ──────────────────────────────────────────────────

pub fn insert_genome(
    conn: &Connection,
    filename: &str,
    format: &str,
    snp_count: i64,
    build: Option<&str>,
) -> Result<i64, AppError> {
    conn.execute(
        "INSERT INTO genomes (filename, format, imported_at, snp_count, build)
         VALUES (?1, ?2, datetime('now'), ?3, ?4)",
        rusqlite::params![filename, format, snp_count, build],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_genomes(conn: &Connection) -> Result<Vec<Genome>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, filename, format, imported_at, snp_count, build FROM genomes ORDER BY imported_at DESC",
    )?;

    let genomes = stmt
        .query_map([], |row| {
            Ok(Genome {
                id: row.get(0)?,
                filename: row.get(1)?,
                format: row.get(2)?,
                imported_at: row.get(3)?,
                snp_count: row.get(4)?,
                build: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(genomes)
}

pub fn get_genome(conn: &Connection, id: i64) -> Result<Genome, AppError> {
    conn.query_row(
        "SELECT id, filename, format, imported_at, snp_count, build FROM genomes WHERE id = ?1",
        [id],
        |row| {
            Ok(Genome {
                id: row.get(0)?,
                filename: row.get(1)?,
                format: row.get(2)?,
                imported_at: row.get(3)?,
                snp_count: row.get(4)?,
                build: row.get(5)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            AppError::NotFound(format!("Genome with id {} not found", id))
        }
        other => AppError::Database(other.to_string()),
    })
}

pub fn delete_genome(conn: &Connection, id: i64) -> Result<(), AppError> {
    let affected = conn.execute("DELETE FROM genomes WHERE id = ?1", [id])?;
    if affected == 0 {
        return Err(AppError::NotFound(format!(
            "Genome with id {} not found",
            id
        )));
    }
    Ok(())
}

// ── SNP Operations ───────────────────────────────────────────────

pub fn insert_snps_batch(
    conn: &Connection,
    genome_id: i64,
    snps: &[SnpRow],
) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;

    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO snps (genome_id, rsid, chromosome, position, genotype)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        for snp in snps {
            stmt.execute(rusqlite::params![
                genome_id,
                snp.rsid,
                snp.chromosome,
                snp.position,
                snp.genotype,
            ])?;
        }
    }

    tx.commit()?;
    Ok(())
}

pub fn get_snp_count_by_chromosome(
    conn: &Connection,
    genome_id: i64,
) -> Result<Vec<ChromosomeCount>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT chromosome, COUNT(*) as count FROM snps
         WHERE genome_id = ?1
         GROUP BY chromosome
         ORDER BY
           CASE
             WHEN chromosome = 'X' THEN 23
             WHEN chromosome = 'Y' THEN 24
             WHEN chromosome = 'MT' THEN 25
             ELSE CAST(chromosome AS INTEGER)
           END",
    )?;

    let counts = stmt
        .query_map([genome_id], |row| {
            Ok(ChromosomeCount {
                chromosome: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(counts)
}

pub fn get_snps_paginated(
    conn: &Connection,
    genome_id: i64,
    offset: i64,
    limit: i64,
    search: Option<&str>,
    chromosome_filter: Option<&str>,
) -> Result<(Vec<SnpRow>, i64), AppError> {
    // Build WHERE clause dynamically
    let mut where_clauses = vec!["genome_id = ?1".to_string()];
    let mut param_index = 2;
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(genome_id)];

    if let Some(search_term) = search {
        if !search_term.is_empty() {
            where_clauses.push(format!("rsid LIKE ?{}", param_index));
            params.push(Box::new(format!("%{}%", search_term)));
            param_index += 1;
        }
    }

    if let Some(chr) = chromosome_filter {
        if !chr.is_empty() {
            where_clauses.push(format!("chromosome = ?{}", param_index));
            params.push(Box::new(chr.to_string()));
            param_index += 1;
        }
    }

    let where_sql = where_clauses.join(" AND ");

    // Get total count
    let count_sql = format!("SELECT COUNT(*) FROM snps WHERE {}", where_sql);
    let total: i64 = {
        let mut stmt = conn.prepare(&count_sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        stmt.query_row(param_refs.as_slice(), |row| row.get(0))?
    };

    // Get paginated rows
    let query_sql = format!(
        "SELECT id, genome_id, rsid, chromosome, position, genotype
         FROM snps WHERE {}
         ORDER BY chromosome, position
         LIMIT ?{} OFFSET ?{}",
        where_sql, param_index, param_index + 1
    );

    params.push(Box::new(limit));
    params.push(Box::new(offset));

    let mut stmt = conn.prepare(&query_sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let rows = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok(SnpRow {
                id: row.get(0)?,
                genome_id: row.get(1)?,
                rsid: row.get(2)?,
                chromosome: row.get(3)?,
                position: row.get(4)?,
                genotype: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok((rows, total))
}

pub fn get_snp_by_rsid(
    conn: &Connection,
    genome_id: i64,
    rsid: &str,
) -> Result<SnpRow, AppError> {
    conn.query_row(
        "SELECT id, genome_id, rsid, chromosome, position, genotype
         FROM snps WHERE genome_id = ?1 AND rsid = ?2",
        rusqlite::params![genome_id, rsid],
        |row| {
            Ok(SnpRow {
                id: row.get(0)?,
                genome_id: row.get(1)?,
                rsid: row.get(2)?,
                chromosome: row.get(3)?,
                position: row.get(4)?,
                genotype: row.get(5)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            AppError::NotFound(format!("SNP {} not found for genome {}", rsid, genome_id))
        }
        other => AppError::Database(other.to_string()),
    })
}

// ── Annotation Operations ────────────────────────────────────────

pub fn upsert_annotations(conn: &Connection, annotations: &[Annotation]) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;

    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO annotations (rsid, gene, clinical_significance, condition, review_status, allele_frequency, source, last_updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))
             ON CONFLICT(rsid) DO UPDATE SET
               gene = excluded.gene,
               clinical_significance = excluded.clinical_significance,
               condition = excluded.condition,
               review_status = excluded.review_status,
               allele_frequency = excluded.allele_frequency,
               source = excluded.source,
               last_updated = datetime('now')",
        )?;

        for ann in annotations {
            stmt.execute(rusqlite::params![
                ann.rsid,
                ann.gene,
                ann.clinical_significance,
                ann.condition,
                ann.review_status,
                ann.allele_frequency,
                ann.source,
            ])?;
        }
    }

    tx.commit()?;
    Ok(())
}

pub fn get_annotations_for_genome(
    conn: &Connection,
    genome_id: i64,
) -> Result<Vec<AnnotatedSnp>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT s.rsid, s.chromosome, s.position, s.genotype,
                a.gene, a.clinical_significance, a.condition,
                a.review_status, a.allele_frequency, a.source
         FROM snps s
         INNER JOIN annotations a ON s.rsid = a.rsid
         WHERE s.genome_id = ?1
         ORDER BY a.clinical_significance, s.chromosome, s.position",
    )?;

    let results = stmt
        .query_map([genome_id], |row| {
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
        .collect::<Result<Vec<_>, _>>()?;

    Ok(results)
}

pub fn get_annotations_by_significance(
    conn: &Connection,
    genome_id: i64,
    significance: &str,
) -> Result<Vec<AnnotatedSnp>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT s.rsid, s.chromosome, s.position, s.genotype,
                a.gene, a.clinical_significance, a.condition,
                a.review_status, a.allele_frequency, a.source
         FROM snps s
         INNER JOIN annotations a ON s.rsid = a.rsid
         WHERE s.genome_id = ?1 AND a.clinical_significance LIKE ?2
         ORDER BY s.chromosome, s.position",
    )?;

    let pattern = format!("%{}%", significance);
    let results = stmt
        .query_map(rusqlite::params![genome_id, pattern], |row| {
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
        .collect::<Result<Vec<_>, _>>()?;

    Ok(results)
}

// ── Reference Database Structures ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceStatus {
    pub source: String,
    pub status: String,
    pub downloaded_at: Option<String>,
    pub parsed_at: Option<String>,
    pub record_count: i64,
    pub file_size_bytes: i64,
    pub error_message: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GwasAssociation {
    pub id: Option<i64>,
    pub rsid: String,
    pub trait_name: String,
    pub p_value: Option<f64>,
    pub odds_ratio: Option<f64>,
    pub risk_allele: Option<String>,
    pub study_accession: Option<String>,
    pub pubmed_id: Option<String>,
    pub sample_size: Option<i64>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnpediaEntry {
    pub rsid: String,
    pub genotype: String,
    pub magnitude: Option<f64>,
    pub repute: Option<String>,
    pub summary: Option<String>,
}

// ── Reference Status Operations ─────────────────────────────────

pub fn upsert_reference_status(
    conn: &Connection,
    source: &str,
    status: &str,
    record_count: i64,
    file_size: i64,
    error_msg: Option<&str>,
    version: Option<&str>,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let downloaded_at = if status == "downloaded" || status == "parsed" || status == "complete" {
        Some(now.clone())
    } else {
        None
    };
    let parsed_at = if status == "parsed" || status == "complete" {
        Some(now)
    } else {
        None
    };

    conn.execute(
        "INSERT INTO reference_status (source, status, downloaded_at, parsed_at, record_count, file_size_bytes, error_message, version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(source) DO UPDATE SET
           status = excluded.status,
           downloaded_at = COALESCE(excluded.downloaded_at, reference_status.downloaded_at),
           parsed_at = COALESCE(excluded.parsed_at, reference_status.parsed_at),
           record_count = excluded.record_count,
           file_size_bytes = excluded.file_size_bytes,
           error_message = excluded.error_message,
           version = COALESCE(excluded.version, reference_status.version)",
        rusqlite::params![source, status, downloaded_at, parsed_at, record_count, file_size, error_msg, version],
    )?;

    Ok(())
}

pub fn get_all_reference_status(conn: &Connection) -> Result<Vec<ReferenceStatus>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT source, status, downloaded_at, parsed_at, record_count, file_size_bytes, error_message, version
         FROM reference_status ORDER BY source",
    )?;

    let results = stmt
        .query_map([], |row| {
            Ok(ReferenceStatus {
                source: row.get(0)?,
                status: row.get(1)?,
                downloaded_at: row.get(2)?,
                parsed_at: row.get(3)?,
                record_count: row.get::<_, i64>(4).unwrap_or(0),
                file_size_bytes: row.get::<_, i64>(5).unwrap_or(0),
                error_message: row.get(6)?,
                version: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(results)
}

pub fn get_reference_status(
    conn: &Connection,
    source: &str,
) -> Result<Option<ReferenceStatus>, AppError> {
    let result = conn.query_row(
        "SELECT source, status, downloaded_at, parsed_at, record_count, file_size_bytes, error_message, version
         FROM reference_status WHERE source = ?1",
        [source],
        |row| {
            Ok(ReferenceStatus {
                source: row.get(0)?,
                status: row.get(1)?,
                downloaded_at: row.get(2)?,
                parsed_at: row.get(3)?,
                record_count: row.get::<_, i64>(4).unwrap_or(0),
                file_size_bytes: row.get::<_, i64>(5).unwrap_or(0),
                error_message: row.get(6)?,
                version: row.get(7)?,
            })
        },
    );

    match result {
        Ok(status) => Ok(Some(status)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e.to_string())),
    }
}

// ── GWAS Operations ─────────────────────────────────────────────

pub fn insert_gwas_batch(
    conn: &Connection,
    associations: &[GwasAssociation],
) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;

    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO gwas_associations (rsid, trait_name, p_value, odds_ratio, risk_allele, study_accession, pubmed_id, sample_size, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;

        for assoc in associations {
            stmt.execute(rusqlite::params![
                assoc.rsid,
                assoc.trait_name,
                assoc.p_value,
                assoc.odds_ratio,
                assoc.risk_allele,
                assoc.study_accession,
                assoc.pubmed_id,
                assoc.sample_size,
                assoc.source,
            ])?;
        }
    }

    tx.commit()?;
    Ok(())
}

pub fn get_gwas_for_rsids(
    conn: &Connection,
    rsids: &[String],
) -> Result<Vec<GwasAssociation>, AppError> {
    if rsids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders: Vec<String> = (1..=rsids.len()).map(|i| format!("?{}", i)).collect();
    let sql = format!(
        "SELECT id, rsid, trait_name, p_value, odds_ratio, risk_allele, study_accession, pubmed_id, sample_size, source
         FROM gwas_associations WHERE rsid IN ({})",
        placeholders.join(", ")
    );

    let mut stmt = conn.prepare(&sql)?;
    let params: Vec<&dyn rusqlite::types::ToSql> = rsids
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let results = stmt
        .query_map(params.as_slice(), |row| {
            Ok(GwasAssociation {
                id: row.get(0)?,
                rsid: row.get(1)?,
                trait_name: row.get(2)?,
                p_value: row.get(3)?,
                odds_ratio: row.get(4)?,
                risk_allele: row.get(5)?,
                study_accession: row.get(6)?,
                pubmed_id: row.get(7)?,
                sample_size: row.get(8)?,
                source: row.get::<_, Option<String>>(9)?
                    .unwrap_or_else(|| "GWAS Catalog".to_string()),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(results)
}

// ── SNPedia Operations ──────────────────────────────────────────

pub fn insert_snpedia_batch(
    conn: &Connection,
    entries: &[SnpediaEntry],
) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;

    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO snpedia_entries (rsid, genotype, magnitude, repute, summary)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(rsid, genotype) DO UPDATE SET
               magnitude = excluded.magnitude,
               repute = excluded.repute,
               summary = excluded.summary",
        )?;

        for entry in entries {
            stmt.execute(rusqlite::params![
                entry.rsid,
                entry.genotype,
                entry.magnitude,
                entry.repute,
                entry.summary,
            ])?;
        }
    }

    tx.commit()?;
    Ok(())
}

pub fn get_snpedia_for_genome(
    conn: &Connection,
    genome_id: i64,
) -> Result<Vec<SnpediaEntry>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT se.rsid, se.genotype, se.magnitude, se.repute, se.summary
         FROM snpedia_entries se
         INNER JOIN snps s ON se.rsid = s.rsid
         WHERE s.genome_id = ?1
         GROUP BY se.rsid, se.genotype
         ORDER BY se.magnitude DESC NULLS LAST",
    )?;

    let results = stmt
        .query_map([genome_id], |row| {
            Ok(SnpediaEntry {
                rsid: row.get(0)?,
                genotype: row.get(1)?,
                magnitude: row.get(2)?,
                repute: row.get(3)?,
                summary: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(results)
}

// ── User RSIDs ──────────────────────────────────────────────────

pub fn get_all_user_rsids(
    conn: &Connection,
    genome_id: i64,
) -> Result<HashSet<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT rsid FROM snps WHERE genome_id = ?1",
    )?;

    let rsids: HashSet<String> = stmt
        .query_map([genome_id], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rsids)
}

// ── Reference Data Cleanup ──────────────────────────────────────

pub fn clear_reference_data(conn: &Connection, source: &str) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;

    match source {
        "clinvar" => {
            tx.execute("DELETE FROM annotations WHERE source = 'ClinVar'", [])?;
        }
        "gwas_catalog" => {
            tx.execute("DELETE FROM gwas_associations", [])?;
        }
        "snpedia" => {
            tx.execute("DELETE FROM snpedia_entries", [])?;
        }
        _ => {
            return Err(AppError::Parse(format!("Unknown reference source: {}", source)));
        }
    }

    tx.execute("DELETE FROM reference_status WHERE source = ?1", [source])?;
    tx.commit()?;

    Ok(())
}

// ── Count Operations ────────────────────────────────────────────

pub fn count_annotations(conn: &Connection) -> Result<i64, AppError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM annotations",
        [],
        |row| row.get(0),
    )?;
    Ok(count)
}

pub fn count_gwas(conn: &Connection) -> Result<i64, AppError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM gwas_associations",
        [],
        |row| row.get(0),
    )?;
    Ok(count)
}

pub fn count_snpedia(conn: &Connection) -> Result<i64, AppError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM snpedia_entries",
        [],
        |row| row.get(0),
    )?;
    Ok(count)
}

// ── Research Digest Operations ──────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchArticleRow {
    pub id: String,
    pub title: String,
    pub abstract_text: Option<String>,
    pub source: String,
    pub published_date: Option<String>,
    pub relevant_rsids: String, // JSON array stored as text
    pub fetched_date: String,
}

/// Returns the most notable rsIDs from a user's genome -- those that have
/// annotations in the annotations table, appear in gwas_associations, or
/// are present in snpedia_entries. Capped at `limit`.
pub fn get_notable_rsids(
    conn: &Connection,
    genome_id: i64,
    limit: usize,
) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT s.rsid FROM snps s
         LEFT JOIN annotations a ON s.rsid = a.rsid
         LEFT JOIN gwas_associations g ON s.rsid = g.rsid
         LEFT JOIN snpedia_entries se ON s.rsid = se.rsid
         WHERE s.genome_id = ?1
           AND (a.rsid IS NOT NULL OR g.rsid IS NOT NULL OR se.rsid IS NOT NULL)
         LIMIT ?2",
    )?;

    let rsids: Vec<String> = stmt
        .query_map(rusqlite::params![genome_id, limit as i64], |row| {
            row.get::<_, String>(0)
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rsids)
}

/// Insert a research article if it does not already exist.
/// Returns true if newly inserted, false if it already existed.
pub fn insert_research_article(
    conn: &Connection,
    article: &ResearchArticleRow,
) -> Result<bool, AppError> {
    let affected = conn.execute(
        "INSERT OR IGNORE INTO research_articles (id, title, abstract_text, source, published_date, relevant_rsids, fetched_date)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            article.id,
            article.title,
            article.abstract_text,
            article.source,
            article.published_date,
            article.relevant_rsids,
            article.fetched_date,
        ],
    )?;
    Ok(affected > 0)
}

/// Store the match between an article and a genome (which rsIDs matched, relevance score).
pub fn upsert_article_genome_match(
    conn: &Connection,
    article_id: &str,
    genome_id: i64,
    matched_rsids_json: &str,
    relevance_score: f64,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO article_genome_matches (article_id, genome_id, matched_rsids, relevance_score)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(article_id, genome_id) DO UPDATE SET
           matched_rsids = excluded.matched_rsids,
           relevance_score = excluded.relevance_score",
        rusqlite::params![article_id, genome_id, matched_rsids_json, relevance_score],
    )?;
    Ok(())
}

/// Get research articles matched to a specific genome, ordered by relevance.
pub fn get_research_articles(
    conn: &Connection,
    genome_id: i64,
    limit: i64,
) -> Result<Vec<(ResearchArticleRow, String, f64)>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT ra.id, ra.title, ra.abstract_text, ra.source, ra.published_date,
                ra.relevant_rsids, ra.fetched_date,
                agm.matched_rsids, agm.relevance_score
         FROM research_articles ra
         INNER JOIN article_genome_matches agm ON ra.id = agm.article_id
         WHERE agm.genome_id = ?1
         ORDER BY ra.fetched_date DESC, agm.relevance_score DESC
         LIMIT ?2",
    )?;

    let results = stmt
        .query_map(rusqlite::params![genome_id, limit], |row| {
            Ok((
                ResearchArticleRow {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    abstract_text: row.get(2)?,
                    source: row.get(3)?,
                    published_date: row.get(4)?,
                    relevant_rsids: row.get(5)?,
                    fetched_date: row.get(6)?,
                },
                row.get::<_, String>(7)?, // matched_rsids JSON
                row.get::<_, f64>(8)?,    // relevance_score
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(results)
}

/// Get the most recent scan date from research_articles.
pub fn get_last_scan_date(conn: &Connection) -> Result<Option<String>, AppError> {
    let result = conn.query_row(
        "SELECT MAX(fetched_date) FROM research_articles",
        [],
        |row| row.get::<_, Option<String>>(0),
    )?;
    Ok(result)
}

/// Mark articles as seen by setting the seen_at timestamp.
pub fn mark_articles_seen(conn: &Connection, article_ids: &[String]) -> Result<(), AppError> {
    if article_ids.is_empty() {
        return Ok(());
    }

    let placeholders: Vec<String> = (1..=article_ids.len()).map(|i| format!("?{}", i)).collect();
    let sql = format!(
        "UPDATE research_articles SET seen_at = datetime('now') WHERE id IN ({}) AND seen_at IS NULL",
        placeholders.join(", ")
    );

    let mut stmt = conn.prepare(&sql)?;
    let params: Vec<&dyn rusqlite::types::ToSql> = article_ids
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    stmt.execute(params.as_slice())?;
    Ok(())
}

// ── Workbench Structures ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchSession {
    pub id: String,
    pub genome_id: i64,
    pub query: String,
    pub strategy: String,
    pub result_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessageRow {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

// ── Workbench Operations ────────────────────────────────────────

pub fn save_workbench_session(
    conn: &Connection,
    id: &str,
    genome_id: i64,
    query: &str,
    strategy: &str,
    result_json: &str,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT OR REPLACE INTO workbench_sessions (id, genome_id, query, strategy, result_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, genome_id, query, strategy, result_json, now],
    )?;
    Ok(())
}

pub fn get_workbench_sessions(
    conn: &Connection,
    genome_id: i64,
    limit: i64,
) -> Result<Vec<WorkbenchSession>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, genome_id, query, strategy, result_json, created_at
         FROM workbench_sessions
         WHERE genome_id = ?1
         ORDER BY created_at DESC
         LIMIT ?2",
    )?;

    let results = stmt
        .query_map(rusqlite::params![genome_id, limit], |row| {
            Ok(WorkbenchSession {
                id: row.get(0)?,
                genome_id: row.get(1)?,
                query: row.get(2)?,
                strategy: row.get(3)?,
                result_json: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(results)
}

pub fn get_workbench_session(
    conn: &Connection,
    id: &str,
) -> Result<Option<WorkbenchSession>, AppError> {
    let result = conn.query_row(
        "SELECT id, genome_id, query, strategy, result_json, created_at
         FROM workbench_sessions WHERE id = ?1",
        [id],
        |row| {
            Ok(WorkbenchSession {
                id: row.get(0)?,
                genome_id: row.get(1)?,
                query: row.get(2)?,
                strategy: row.get(3)?,
                result_json: row.get(4)?,
                created_at: row.get(5)?,
            })
        },
    );

    match result {
        Ok(session) => Ok(Some(session)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e.to_string())),
    }
}

pub fn save_chat_message(
    conn: &Connection,
    session_id: &str,
    role: &str,
    content: &str,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO workbench_chats (session_id, role, content, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![session_id, role, content, now],
    )?;
    Ok(())
}

pub fn get_chat_messages(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<ChatMessageRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, role, content, created_at
         FROM workbench_chats
         WHERE session_id = ?1
         ORDER BY created_at ASC",
    )?;

    let results = stmt
        .query_map([session_id], |row| {
            Ok(ChatMessageRow {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(results)
}

/// Count articles that are matched to this genome but haven't been seen yet.
pub fn count_unseen_articles(conn: &Connection, genome_id: i64) -> Result<i64, AppError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM research_articles ra
         INNER JOIN article_genome_matches agm ON ra.id = agm.article_id
         WHERE agm.genome_id = ?1 AND ra.seen_at IS NULL",
        [genome_id],
        |row| row.get(0),
    )?;
    Ok(count)
}
