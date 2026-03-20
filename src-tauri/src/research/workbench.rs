use std::collections::HashSet;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::db::queries::SnpRow;
use crate::db::Database;
use crate::error::AppError;
use crate::research::intent::{
    extract_rsid, parse_question, QueryIntent,
    CONDITION_KEYWORDS, DRUG_KEYWORDS, KNOWN_GENES, TRAIT_KEYWORDS, CARRIER_KEYWORDS,
};

// ── Structs ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResearchStrategy {
    SnpsFirst,
    ResearchFirst,
}

impl std::fmt::Display for ResearchStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResearchStrategy::SnpsFirst => write!(f, "snps_first"),
            ResearchStrategy::ResearchFirst => write!(f, "research_first"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchProgress {
    pub step: String,
    pub progress: f64,
    pub message: String,
    pub strategy: Option<String>,
    pub partial_snps: Option<Vec<EvidenceSnp>>,
    pub partial_articles: Option<Vec<WorkbenchArticle>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceSnp {
    pub rsid: String,
    pub chromosome: String,
    pub position: i64,
    pub genotype: String,
    pub gene: Option<String>,
    pub why_selected: String,
    pub clinvar: Option<ClinvarEvidence>,
    pub gwas: Vec<GwasEvidence>,
    pub snpedia: Option<SnpediaEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClinvarEvidence {
    pub clinical_significance: String,
    pub condition: String,
    pub review_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GwasEvidence {
    pub trait_name: String,
    pub p_value: Option<f64>,
    pub odds_ratio: Option<f64>,
    pub risk_allele: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnpediaEvidence {
    pub summary: String,
    pub magnitude: Option<f64>,
    pub repute: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchArticle {
    pub pmid: String,
    pub title: String,
    pub authors: String,
    pub journal: String,
    pub published_date: String,
    pub url: String,
    pub matched_rsids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchResult {
    pub query: String,
    pub strategy: String,
    pub evidence_snps: Vec<EvidenceSnp>,
    pub articles: Vec<WorkbenchArticle>,
    pub claude_context: String,
}

pub struct PipelineInput {
    pub query: String,
    pub genome_id: i64,
    pub user_rsids: HashSet<String>,
    pub snp_rows: Vec<SnpRow>,
}

// ── Strategy Determination ──────────────────────────────────────

pub fn determine_strategy(query: &str) -> ResearchStrategy {
    let q = query.to_lowercase();

    // Research-first for general/exploratory queries
    let research_keywords = [
        "latest", "research", "study", "studies", "new findings",
        "recent", "published", "paper", "papers", "literature",
        "evidence", "what does science say", "what do studies say",
    ];

    for keyword in &research_keywords {
        if q.contains(keyword) {
            return ResearchStrategy::ResearchFirst;
        }
    }

    // SNPs-first for specific genes/conditions/rsIDs/drugs/traits/carrier queries
    let intent = parse_question(query);
    match intent {
        QueryIntent::RsidLookup(_)
        | QueryIntent::GeneLookup(_)
        | QueryIntent::ConditionRisk(_)
        | QueryIntent::DrugResponse(_)
        | QueryIntent::TraitQuery(_)
        | QueryIntent::CarrierQuery(_) => ResearchStrategy::SnpsFirst,
        _ => ResearchStrategy::ResearchFirst,
    }
}

// ── Pipeline Execution ──────────────────────────────────────────

pub async fn execute_pipeline(
    input: PipelineInput,
    conn_for_sync: &Database,
    progress: impl Fn(WorkbenchProgress),
) -> Result<WorkbenchResult, AppError> {
    let strategy = determine_strategy(&input.query);
    let strategy_str = strategy.to_string();

    progress(WorkbenchProgress {
        step: "starting".to_string(),
        progress: 0.0,
        message: format!("Using {} strategy", strategy_str),
        strategy: Some(strategy_str.clone()),
        partial_snps: None,
        partial_articles: None,
    });

    match strategy {
        ResearchStrategy::SnpsFirst => {
            execute_snps_first(input, conn_for_sync, &progress).await
        }
        ResearchStrategy::ResearchFirst => {
            execute_research_first(input, conn_for_sync, &progress).await
        }
    }
}

// ── SNPs-First Pipeline ─────────────────────────────────────────

async fn execute_snps_first(
    input: PipelineInput,
    conn_for_sync: &Database,
    progress: &impl Fn(WorkbenchProgress),
) -> Result<WorkbenchResult, AppError> {
    progress(WorkbenchProgress {
        step: "parsing".to_string(),
        progress: 0.05,
        message: "Analyzing your query...".to_string(),
        strategy: Some("snps_first".to_string()),
        partial_snps: None,
        partial_articles: None,
    });

    let intent = parse_question(&input.query);

    // Step 1-2: Find matching SNPs based on intent
    let mut evidence_snps: Vec<EvidenceSnp> = Vec::new();

    {
        let conn = conn_for_sync.0.lock().map_err(|e| {
            AppError::Database(format!("Failed to acquire database lock: {}", e))
        })?;

        match &intent {
            QueryIntent::RsidLookup(rsid) => {
                // Direct lookup
                if let Ok(snp) = conn.query_row(
                    "SELECT rsid, chromosome, position, genotype FROM snps WHERE genome_id = ?1 AND rsid = ?2",
                    rusqlite::params![input.genome_id, rsid],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, String>(3)?)),
                ) {
                    evidence_snps.push(EvidenceSnp {
                        rsid: snp.0,
                        chromosome: snp.1,
                        position: snp.2,
                        genotype: snp.3,
                        gene: None,
                        why_selected: format!("Directly queried: {}", rsid),
                        clinvar: None,
                        gwas: Vec::new(),
                        snpedia: None,
                    });
                }
            }
            QueryIntent::GeneLookup(gene) => {
                let mut stmt = conn.prepare(
                    "SELECT s.rsid, s.chromosome, s.position, s.genotype, a.gene
                     FROM snps s
                     INNER JOIN annotations a ON s.rsid = a.rsid
                     WHERE s.genome_id = ?1 AND UPPER(a.gene) = UPPER(?2)
                     ORDER BY s.position
                     LIMIT 50",
                )?;
                let rows: Vec<(String, String, i64, String, Option<String>)> = stmt
                    .query_map(rusqlite::params![input.genome_id, gene], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
                    })?
                    .filter_map(|r| r.ok())
                    .collect();
                for (rsid, chr, pos, gt, g) in rows {
                    evidence_snps.push(EvidenceSnp {
                        rsid,
                        chromosome: chr,
                        position: pos,
                        genotype: gt,
                        gene: g,
                        why_selected: format!("Variant in gene {}", gene),
                        clinvar: None,
                        gwas: Vec::new(),
                        snpedia: None,
                    });
                }
            }
            QueryIntent::ConditionRisk(condition) => {
                let pattern = if condition == "all" {
                    "%".to_string()
                } else {
                    format!("%{}%", condition.to_lowercase())
                };
                let mut stmt = conn.prepare(
                    "SELECT DISTINCT s.rsid, s.chromosome, s.position, s.genotype, a.gene
                     FROM snps s
                     LEFT JOIN annotations a ON s.rsid = a.rsid
                     LEFT JOIN gwas_associations g ON s.rsid = g.rsid
                     WHERE s.genome_id = ?1
                       AND (LOWER(a.condition) LIKE ?2 OR LOWER(g.trait_name) LIKE ?2)
                     ORDER BY s.position
                     LIMIT 50",
                )?;
                let rows: Vec<(String, String, i64, String, Option<String>)> = stmt
                    .query_map(rusqlite::params![input.genome_id, pattern], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
                    })?
                    .filter_map(|r| r.ok())
                    .collect();
                for (rsid, chr, pos, gt, g) in rows {
                    evidence_snps.push(EvidenceSnp {
                        rsid,
                        chromosome: chr,
                        position: pos,
                        genotype: gt,
                        gene: g,
                        why_selected: format!("Associated with {}", condition),
                        clinvar: None,
                        gwas: Vec::new(),
                        snpedia: None,
                    });
                }
            }
            QueryIntent::DrugResponse(drug) => {
                let pharma_genes: &[(&str, &[&str])] = &[
                    ("CYP2D6", &["codeine", "tramadol", "tamoxifen", "fluoxetine", "amitriptyline", "metoprolol"]),
                    ("CYP2C19", &["clopidogrel", "omeprazole", "pantoprazole", "lansoprazole", "citalopram", "escitalopram", "voriconazole"]),
                    ("CYP2C9", &["warfarin", "ibuprofen"]),
                    ("CYP3A4", &["tacrolimus", "simvastatin", "statin"]),
                    ("VKORC1", &["warfarin"]),
                    ("SLCO1B1", &["simvastatin", "statin"]),
                    ("DPYD", &["fluorouracil", "5-fu"]),
                    ("TPMT", &["azathioprine", "mercaptopurine"]),
                    ("UGT1A1", &["irinotecan"]),
                    ("CYP1A2", &["caffeine"]),
                ];
                let drug_lower = drug.to_lowercase();
                let genes: Vec<&str> = if drug == "all" {
                    pharma_genes.iter().map(|(g, _)| *g).collect()
                } else {
                    pharma_genes.iter()
                        .filter(|(_, drugs)| drugs.iter().any(|d| *d == drug_lower.as_str()))
                        .map(|(g, _)| *g)
                        .collect()
                };
                for gene in genes {
                    let mut stmt = conn.prepare(
                        "SELECT s.rsid, s.chromosome, s.position, s.genotype, a.gene
                         FROM snps s
                         INNER JOIN annotations a ON s.rsid = a.rsid
                         WHERE s.genome_id = ?1 AND UPPER(a.gene) = UPPER(?2)
                         ORDER BY s.position
                         LIMIT 50",
                    )?;
                    let rows: Vec<(String, String, i64, String, Option<String>)> = stmt
                        .query_map(rusqlite::params![input.genome_id, gene], |row| {
                            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
                        })?
                        .filter_map(|r| r.ok())
                        .collect();
                    for (rsid, chr, pos, gt, g) in rows {
                        if !evidence_snps.iter().any(|s| s.rsid == rsid) {
                            evidence_snps.push(EvidenceSnp {
                                rsid,
                                chromosome: chr,
                                position: pos,
                                genotype: gt,
                                gene: g,
                                why_selected: format!("Pharmacogene {} related to {}", gene, drug),
                                clinvar: None,
                                gwas: Vec::new(),
                                snpedia: None,
                            });
                        }
                    }
                }
            }
            QueryIntent::TraitQuery(trait_name) => {
                let pattern = format!("%{}%", trait_name.to_lowercase());
                let mut stmt = conn.prepare(
                    "SELECT DISTINCT s.rsid, s.chromosome, s.position, s.genotype, a.gene
                     FROM snps s
                     LEFT JOIN annotations a ON s.rsid = a.rsid
                     LEFT JOIN gwas_associations g ON s.rsid = g.rsid
                     LEFT JOIN snpedia_entries se ON s.rsid = se.rsid
                     WHERE s.genome_id = ?1
                       AND (LOWER(g.trait_name) LIKE ?2 OR LOWER(se.summary) LIKE ?2 OR LOWER(a.condition) LIKE ?2)
                     ORDER BY s.position
                     LIMIT 50",
                )?;
                let rows: Vec<(String, String, i64, String, Option<String>)> = stmt
                    .query_map(rusqlite::params![input.genome_id, pattern], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
                    })?
                    .filter_map(|r| r.ok())
                    .collect();
                for (rsid, chr, pos, gt, g) in rows {
                    evidence_snps.push(EvidenceSnp {
                        rsid,
                        chromosome: chr,
                        position: pos,
                        genotype: gt,
                        gene: g,
                        why_selected: format!("Associated with trait: {}", trait_name),
                        clinvar: None,
                        gwas: Vec::new(),
                        snpedia: None,
                    });
                }
            }
            QueryIntent::CarrierQuery(condition) => {
                let pattern = if condition == "all" {
                    "%".to_string()
                } else {
                    format!("%{}%", condition.to_lowercase())
                };
                let mut stmt = conn.prepare(
                    "SELECT DISTINCT s.rsid, s.chromosome, s.position, s.genotype, a.gene
                     FROM snps s
                     INNER JOIN annotations a ON s.rsid = a.rsid
                     WHERE s.genome_id = ?1
                       AND LOWER(a.condition) LIKE ?2
                       AND a.clinical_significance IS NOT NULL
                     ORDER BY s.position
                     LIMIT 50",
                )?;
                let rows: Vec<(String, String, i64, String, Option<String>)> = stmt
                    .query_map(rusqlite::params![input.genome_id, pattern], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
                    })?
                    .filter_map(|r| r.ok())
                    .collect();
                for (rsid, chr, pos, gt, g) in rows {
                    evidence_snps.push(EvidenceSnp {
                        rsid,
                        chromosome: chr,
                        position: pos,
                        genotype: gt,
                        gene: g,
                        why_selected: format!("Carrier screening for {}", condition),
                        clinvar: None,
                        gwas: Vec::new(),
                        snpedia: None,
                    });
                }
            }
            _ => {
                // For general/unknown, use search terms from query
                let search_terms = extract_search_terms(&input.query);
                for term in &search_terms {
                    let pattern = format!("%{}%", term.to_lowercase());
                    let mut stmt = conn.prepare(
                        "SELECT DISTINCT s.rsid, s.chromosome, s.position, s.genotype, a.gene
                         FROM snps s
                         LEFT JOIN annotations a ON s.rsid = a.rsid
                         LEFT JOIN gwas_associations g ON s.rsid = g.rsid
                         WHERE s.genome_id = ?1
                           AND (LOWER(a.condition) LIKE ?2 OR LOWER(a.gene) LIKE ?2 OR LOWER(g.trait_name) LIKE ?2)
                         LIMIT 20",
                    )?;
                    let rows: Vec<(String, String, i64, String, Option<String>)> = stmt
                        .query_map(rusqlite::params![input.genome_id, pattern], |row| {
                            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
                        })?
                        .filter_map(|r| r.ok())
                        .collect();
                    for (rsid, chr, pos, gt, g) in rows {
                        if !evidence_snps.iter().any(|s| s.rsid == rsid) {
                            evidence_snps.push(EvidenceSnp {
                                rsid,
                                chromosome: chr,
                                position: pos,
                                genotype: gt,
                                gene: g,
                                why_selected: format!("Matched search term: {}", term),
                                clinvar: None,
                                gwas: Vec::new(),
                                snpedia: None,
                            });
                        }
                    }
                }
            }
        }
    }

    // Cap at 50
    evidence_snps.truncate(50);

    // Step 3: Send partial SNPs
    progress(WorkbenchProgress {
        step: "snps_found".to_string(),
        progress: 0.3,
        message: format!("Found {} relevant variants", evidence_snps.len()),
        strategy: Some("snps_first".to_string()),
        partial_snps: Some(evidence_snps.clone()),
        partial_articles: None,
    });

    // Step 4-7: Look up evidence for each SNP
    {
        let conn = conn_for_sync.0.lock().map_err(|e| {
            AppError::Database(format!("Failed to acquire database lock: {}", e))
        })?;

        for snp in &mut evidence_snps {
            // ClinVar annotations
            if let Ok(ann) = conn.query_row(
                "SELECT clinical_significance, condition, review_status, gene
                 FROM annotations WHERE rsid = ?1",
                [&snp.rsid],
                |row| Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                )),
            ) {
                if ann.0.is_some() || ann.1.is_some() {
                    snp.clinvar = Some(ClinvarEvidence {
                        clinical_significance: ann.0.unwrap_or_default(),
                        condition: ann.1.unwrap_or_default(),
                        review_status: ann.2,
                    });
                }
                if snp.gene.is_none() {
                    snp.gene = ann.3;
                }
            }

            // GWAS associations
            let mut gwas_stmt = conn.prepare(
                "SELECT trait_name, p_value, odds_ratio, risk_allele
                 FROM gwas_associations WHERE rsid = ?1",
            )?;
            let gwas_rows: Vec<GwasEvidence> = gwas_stmt
                .query_map([&snp.rsid], |row| {
                    Ok(GwasEvidence {
                        trait_name: row.get(0)?,
                        p_value: row.get(1)?,
                        odds_ratio: row.get(2)?,
                        risk_allele: row.get(3)?,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
            snp.gwas = gwas_rows;

            // SNPedia
            if let Ok(se) = conn.query_row(
                "SELECT summary, magnitude, repute
                 FROM snpedia_entries WHERE rsid = ?1 AND genotype = ?2",
                rusqlite::params![snp.rsid, snp.genotype],
                |row| Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<f64>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                )),
            ) {
                if let Some(summary) = se.0 {
                    snp.snpedia = Some(SnpediaEvidence {
                        summary,
                        magnitude: se.1,
                        repute: se.2,
                    });
                }
            }
        }
    }

    // Send updated SNPs with evidence
    progress(WorkbenchProgress {
        step: "evidence_loaded".to_string(),
        progress: 0.6,
        message: "Evidence loaded for all variants".to_string(),
        strategy: Some("snps_first".to_string()),
        partial_snps: Some(evidence_snps.clone()),
        partial_articles: None,
    });

    // Step 8: Fetch PubMed articles using matched rsIDs
    let search_rsids: Vec<String> = evidence_snps
        .iter()
        .map(|s| s.rsid.clone())
        .collect();

    let articles = fetch_pubmed_for_rsids(&search_rsids, &progress, "snps_first").await?;

    // Step 9: Send partial articles
    progress(WorkbenchProgress {
        step: "articles_found".to_string(),
        progress: 0.9,
        message: format!("Found {} research articles", articles.len()),
        strategy: Some("snps_first".to_string()),
        partial_snps: Some(evidence_snps.clone()),
        partial_articles: Some(articles.clone()),
    });

    // Step 10: Build claude_context
    let claude_context = build_claude_context(&input.query, &evidence_snps, &articles);

    Ok(WorkbenchResult {
        query: input.query,
        strategy: "snps_first".to_string(),
        evidence_snps,
        articles,
        claude_context,
    })
}

// ── Research-First Pipeline ─────────────────────────────────────

async fn execute_research_first(
    input: PipelineInput,
    conn_for_sync: &Database,
    progress: &impl Fn(WorkbenchProgress),
) -> Result<WorkbenchResult, AppError> {
    // Step 1: Extract search terms
    let search_terms = extract_search_terms(&input.query);

    progress(WorkbenchProgress {
        step: "searching_pubmed".to_string(),
        progress: 0.1,
        message: format!("Searching PubMed for: {}", search_terms.join(", ")),
        strategy: Some("research_first".to_string()),
        partial_snps: None,
        partial_articles: None,
    });

    // Step 2: Search PubMed
    let articles = fetch_pubmed_for_terms(&search_terms, progress, "research_first").await?;

    // Step 3: Send partial articles
    progress(WorkbenchProgress {
        step: "articles_found".to_string(),
        progress: 0.4,
        message: format!("Found {} research articles", articles.len()),
        strategy: Some("research_first".to_string()),
        partial_snps: None,
        partial_articles: Some(articles.clone()),
    });

    // Step 4: Extract rsIDs from article titles
    let mut found_rsids: HashSet<String> = HashSet::new();
    for article in &articles {
        let title_lower = article.title.to_lowercase();
        extract_all_rsids(&title_lower, &mut found_rsids);
    }

    // Step 5: Check which extracted rsIDs the user carries
    let mut evidence_snps: Vec<EvidenceSnp> = Vec::new();

    {
        let conn = conn_for_sync.0.lock().map_err(|e| {
            AppError::Database(format!("Failed to acquire database lock: {}", e))
        })?;

        for rsid in &found_rsids {
            if !input.user_rsids.contains(rsid) {
                continue;
            }
            if let Ok(snp) = conn.query_row(
                "SELECT rsid, chromosome, position, genotype FROM snps WHERE genome_id = ?1 AND rsid = ?2",
                rusqlite::params![input.genome_id, rsid],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, String>(3)?)),
            ) {
                evidence_snps.push(EvidenceSnp {
                    rsid: snp.0,
                    chromosome: snp.1,
                    position: snp.2,
                    genotype: snp.3,
                    gene: None,
                    why_selected: "Mentioned in research articles".to_string(),
                    clinvar: None,
                    gwas: Vec::new(),
                    snpedia: None,
                });
            }
        }

        // Also search for SNPs related to search terms if we didn't find any from articles
        if evidence_snps.is_empty() {
            for term in &search_terms {
                let pattern = format!("%{}%", term.to_lowercase());
                let mut stmt = conn.prepare(
                    "SELECT DISTINCT s.rsid, s.chromosome, s.position, s.genotype, a.gene
                     FROM snps s
                     LEFT JOIN annotations a ON s.rsid = a.rsid
                     LEFT JOIN gwas_associations g ON s.rsid = g.rsid
                     WHERE s.genome_id = ?1
                       AND (LOWER(a.condition) LIKE ?2 OR LOWER(a.gene) LIKE ?2 OR LOWER(g.trait_name) LIKE ?2)
                     LIMIT 20",
                )?;
                let rows: Vec<(String, String, i64, String, Option<String>)> = stmt
                    .query_map(rusqlite::params![input.genome_id, pattern], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
                    })?
                    .filter_map(|r| r.ok())
                    .collect();
                for (rsid, chr, pos, gt, g) in rows {
                    if !evidence_snps.iter().any(|s| s.rsid == rsid) {
                        evidence_snps.push(EvidenceSnp {
                            rsid,
                            chromosome: chr,
                            position: pos,
                            genotype: gt,
                            gene: g,
                            why_selected: format!("Related to search term: {}", term),
                            clinvar: None,
                            gwas: Vec::new(),
                            snpedia: None,
                        });
                    }
                }
            }
        }

        // Cap at 50
        evidence_snps.truncate(50);

        // Step 6: Look up ClinVar/GWAS/SNPedia for carried rsIDs
        for snp in &mut evidence_snps {
            if let Ok(ann) = conn.query_row(
                "SELECT clinical_significance, condition, review_status, gene
                 FROM annotations WHERE rsid = ?1",
                [&snp.rsid],
                |row| Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                )),
            ) {
                if ann.0.is_some() || ann.1.is_some() {
                    snp.clinvar = Some(ClinvarEvidence {
                        clinical_significance: ann.0.unwrap_or_default(),
                        condition: ann.1.unwrap_or_default(),
                        review_status: ann.2,
                    });
                }
                if snp.gene.is_none() {
                    snp.gene = ann.3;
                }
            }

            let mut gwas_stmt = conn.prepare(
                "SELECT trait_name, p_value, odds_ratio, risk_allele
                 FROM gwas_associations WHERE rsid = ?1",
            )?;
            let gwas_rows: Vec<GwasEvidence> = gwas_stmt
                .query_map([&snp.rsid], |row| {
                    Ok(GwasEvidence {
                        trait_name: row.get(0)?,
                        p_value: row.get(1)?,
                        odds_ratio: row.get(2)?,
                        risk_allele: row.get(3)?,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
            snp.gwas = gwas_rows;

            if let Ok(se) = conn.query_row(
                "SELECT summary, magnitude, repute
                 FROM snpedia_entries WHERE rsid = ?1 AND genotype = ?2",
                rusqlite::params![snp.rsid, snp.genotype],
                |row| Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<f64>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                )),
            ) {
                if let Some(summary) = se.0 {
                    snp.snpedia = Some(SnpediaEvidence {
                        summary,
                        magnitude: se.1,
                        repute: se.2,
                    });
                }
            }
        }
    }

    // Step 7-8: Send partial SNPs
    progress(WorkbenchProgress {
        step: "snps_found".to_string(),
        progress: 0.8,
        message: format!("Found {} variants in your genome", evidence_snps.len()),
        strategy: Some("research_first".to_string()),
        partial_snps: Some(evidence_snps.clone()),
        partial_articles: Some(articles.clone()),
    });

    // Step 9: Build claude_context
    let claude_context = build_claude_context(&input.query, &evidence_snps, &articles);

    Ok(WorkbenchResult {
        query: input.query,
        strategy: "research_first".to_string(),
        evidence_snps,
        articles,
        claude_context,
    })
}

// ── PubMed Fetching ─────────────────────────────────────────────

/// NCBI ESearch response structures.
#[derive(Debug, Deserialize)]
struct ESearchResult {
    esearchresult: ESearchData,
}

#[derive(Debug, Deserialize)]
struct ESearchData {
    idlist: Vec<String>,
}

/// Simple URL encoding for query parameters.
fn urlencoded(s: &str) -> String {
    s.replace(' ', "+")
        .replace('/', "%2F")
        .replace('[', "%5B")
        .replace(']', "%5D")
        .replace('(', "%28")
        .replace(')', "%29")
}

async fn fetch_pubmed_for_rsids(
    rsids: &[String],
    progress: &impl Fn(WorkbenchProgress),
    strategy: &str,
) -> Result<Vec<WorkbenchArticle>, AppError> {
    if rsids.is_empty() {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Network(format!("Failed to create HTTP client: {}", e)))?;

    let mut all_articles: Vec<WorkbenchArticle> = Vec::new();
    let mut seen_pmids: HashSet<String> = HashSet::new();

    // Cap to 5 batches (50 rsIDs total)
    let batch_size = 10;
    let batches: Vec<&[String]> = rsids.chunks(batch_size).take(5).collect();

    for (batch_idx, batch) in batches.iter().enumerate() {
        let batch_progress = 0.6 + (batch_idx as f64 / batches.len() as f64) * 0.25;
        progress(WorkbenchProgress {
            step: "fetching_articles".to_string(),
            progress: batch_progress,
            message: format!("Searching PubMed batch {}/{}...", batch_idx + 1, batches.len()),
            strategy: Some(strategy.to_string()),
            partial_snps: None,
            partial_articles: None,
        });

        let query = batch.iter().map(|rsid| rsid.as_str()).collect::<Vec<_>>().join(" OR ");

        let esearch_url = format!(
            "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db=pubmed&term={}&retmax=5&sort=date&retmode=json",
            urlencoded(&query)
        );

        let search_resp = match client.get(&esearch_url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                log::warn!("PubMed search failed for batch {}: {}", batch_idx, e);
                tokio::time::sleep(Duration::from_millis(350)).await;
                continue;
            }
        };

        if !search_resp.status().is_success() {
            tokio::time::sleep(Duration::from_millis(350)).await;
            continue;
        }

        let search_text = match search_resp.text().await {
            Ok(t) => t,
            Err(_) => { tokio::time::sleep(Duration::from_millis(350)).await; continue; }
        };

        let search_result: ESearchResult = match serde_json::from_str(&search_text) {
            Ok(r) => r,
            Err(_) => { tokio::time::sleep(Duration::from_millis(350)).await; continue; }
        };

        let pmids: Vec<String> = search_result.esearchresult.idlist
            .into_iter()
            .filter(|id| !seen_pmids.contains(id))
            .collect();

        if pmids.is_empty() {
            tokio::time::sleep(Duration::from_millis(350)).await;
            continue;
        }

        for id in &pmids { seen_pmids.insert(id.clone()); }

        tokio::time::sleep(Duration::from_millis(350)).await;

        let ids = pmids.join(",");
        let esummary_url = format!(
            "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=pubmed&id={}&retmode=json",
            ids
        );

        let summary_resp = match client.get(&esummary_url).send().await {
            Ok(resp) => resp,
            Err(_) => { tokio::time::sleep(Duration::from_millis(350)).await; continue; }
        };

        if !summary_resp.status().is_success() {
            tokio::time::sleep(Duration::from_millis(350)).await;
            continue;
        }

        let summary_text = match summary_resp.text().await {
            Ok(t) => t,
            Err(_) => { tokio::time::sleep(Duration::from_millis(350)).await; continue; }
        };

        let summary_json: serde_json::Value = match serde_json::from_str(&summary_text) {
            Ok(v) => v,
            Err(_) => { tokio::time::sleep(Duration::from_millis(350)).await; continue; }
        };

        if let Some(result_obj) = summary_json.get("result") {
            for pmid in &pmids {
                if let Some(article_obj) = result_obj.get(pmid) {
                    let title = article_obj.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let journal = article_obj.get("source").and_then(|v| v.as_str()).unwrap_or("PubMed").to_string();
                    let pubdate = article_obj.get("pubdate").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let authors = extract_authors(article_obj);

                    let matched: Vec<String> = batch.iter()
                        .filter(|rsid| title.to_lowercase().contains(&rsid.to_lowercase()))
                        .cloned()
                        .collect();
                    let matched_rsids = if matched.is_empty() { batch.to_vec() } else { matched };

                    all_articles.push(WorkbenchArticle {
                        pmid: pmid.clone(),
                        title,
                        authors,
                        journal,
                        published_date: pubdate,
                        url: format!("https://pubmed.ncbi.nlm.nih.gov/{}/", pmid),
                        matched_rsids,
                    });
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(350)).await;
    }

    Ok(all_articles)
}

async fn fetch_pubmed_for_terms(
    terms: &[String],
    progress: &impl Fn(WorkbenchProgress),
    strategy: &str,
) -> Result<Vec<WorkbenchArticle>, AppError> {
    if terms.is_empty() {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Network(format!("Failed to create HTTP client: {}", e)))?;

    let mut all_articles: Vec<WorkbenchArticle> = Vec::new();
    let mut seen_pmids: HashSet<String> = HashSet::new();

    // Build query from search terms
    let query = terms.join(" AND ");

    progress(WorkbenchProgress {
        step: "fetching_articles".to_string(),
        progress: 0.15,
        message: format!("Searching PubMed for: {}", query),
        strategy: Some(strategy.to_string()),
        partial_snps: None,
        partial_articles: None,
    });

    let esearch_url = format!(
        "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db=pubmed&term={}&retmax=20&sort=date&retmode=json",
        urlencoded(&query)
    );

    let search_resp = match client.get(&esearch_url).send().await {
        Ok(resp) => resp,
        Err(e) => {
            log::warn!("PubMed search failed: {}", e);
            return Ok(Vec::new());
        }
    };

    if !search_resp.status().is_success() {
        return Ok(Vec::new());
    }

    let search_text = search_resp.text().await
        .map_err(|e| AppError::Network(format!("Failed to read response: {}", e)))?;

    let search_result: ESearchResult = match serde_json::from_str(&search_text) {
        Ok(r) => r,
        Err(_) => return Ok(Vec::new()),
    };

    let pmids: Vec<String> = search_result.esearchresult.idlist
        .into_iter()
        .filter(|id| !seen_pmids.contains(id))
        .collect();

    if pmids.is_empty() {
        return Ok(Vec::new());
    }

    for id in &pmids { seen_pmids.insert(id.clone()); }

    tokio::time::sleep(Duration::from_millis(350)).await;

    let ids = pmids.join(",");
    let esummary_url = format!(
        "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=pubmed&id={}&retmode=json",
        ids
    );

    let summary_resp = match client.get(&esummary_url).send().await {
        Ok(resp) => resp,
        Err(e) => {
            log::warn!("PubMed summary failed: {}", e);
            return Ok(Vec::new());
        }
    };

    if !summary_resp.status().is_success() {
        return Ok(Vec::new());
    }

    let summary_text = summary_resp.text().await
        .map_err(|e| AppError::Network(format!("Failed to read response: {}", e)))?;

    let summary_json: serde_json::Value = match serde_json::from_str(&summary_text) {
        Ok(v) => v,
        Err(_) => return Ok(Vec::new()),
    };

    if let Some(result_obj) = summary_json.get("result") {
        for pmid in &pmids {
            if let Some(article_obj) = result_obj.get(pmid) {
                let title = article_obj.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let journal = article_obj.get("source").and_then(|v| v.as_str()).unwrap_or("PubMed").to_string();
                let pubdate = article_obj.get("pubdate").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let authors = extract_authors(article_obj);

                // Find rsIDs mentioned in the title
                let mut matched_rsids = Vec::new();
                let title_lower = title.to_lowercase();
                let mut found = HashSet::new();
                extract_all_rsids(&title_lower, &mut found);
                matched_rsids.extend(found);

                all_articles.push(WorkbenchArticle {
                    pmid: pmid.clone(),
                    title,
                    authors,
                    journal,
                    published_date: pubdate,
                    url: format!("https://pubmed.ncbi.nlm.nih.gov/{}/", pmid),
                    matched_rsids,
                });
            }
        }
    }

    Ok(all_articles)
}

/// Extract first author + "et al" from ESummary article JSON.
fn extract_authors(article_obj: &serde_json::Value) -> String {
    if let Some(authors_arr) = article_obj.get("authors").and_then(|v| v.as_array()) {
        if let Some(first) = authors_arr.first() {
            let name = first.get("name").and_then(|v| v.as_str()).unwrap_or("Unknown");
            if authors_arr.len() > 1 {
                format!("{} et al", name)
            } else {
                name.to_string()
            }
        } else {
            String::new()
        }
    } else {
        String::new()
    }
}

// ── Helpers ─────────────────────────────────────────────────────

fn extract_search_terms(query: &str) -> Vec<String> {
    let q = query.to_lowercase();
    let mut terms = Vec::new();

    // Check for gene names
    for &gene in KNOWN_GENES {
        if q.contains(gene) {
            terms.push(gene.to_uppercase());
        }
    }

    // Check for conditions
    for &(keyword, condition) in CONDITION_KEYWORDS {
        if q.contains(keyword) {
            terms.push(condition.to_string());
        }
    }

    // Check for drugs
    for &(keyword, drug) in DRUG_KEYWORDS {
        if q.contains(keyword) {
            terms.push(drug.to_string());
        }
    }

    // Check for traits
    for &(keyword, trait_name) in TRAIT_KEYWORDS {
        if q.contains(keyword) {
            terms.push(trait_name.to_string());
        }
    }

    // Check for carrier conditions
    for &(keyword, condition) in CARRIER_KEYWORDS {
        if q.contains(keyword) {
            terms.push(condition.to_string());
        }
    }

    // Check for rsIDs
    if let Some(rsid) = extract_rsid(&q) {
        terms.push(rsid);
    }

    // If no specific terms found, use the query words as search terms
    if terms.is_empty() {
        let stop_words: HashSet<&str> = [
            "what", "is", "are", "the", "my", "do", "i", "have", "am", "a",
            "an", "for", "to", "of", "in", "on", "at", "and", "or", "how",
            "does", "about", "me", "tell", "can", "could", "would", "should",
            "with", "this", "that", "these", "those", "from", "by", "was",
            "were", "been", "being", "has", "had", "having", "it", "its",
            "genome", "dna", "genetic", "genetics",
        ].into_iter().collect();

        for word in q.split_whitespace() {
            let clean: String = word.chars().filter(|c| c.is_alphanumeric()).collect();
            if clean.len() > 2 && !stop_words.contains(clean.as_str()) {
                terms.push(clean);
            }
        }
    }

    // Deduplicate
    let mut seen = HashSet::new();
    terms.retain(|t| {
        let key = t.to_lowercase();
        if seen.contains(&key) { false } else { seen.insert(key); true }
    });

    terms
}

fn extract_all_rsids(text: &str, found: &mut HashSet<String>) {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 2 < len {
        if bytes[i] == b'r' && bytes[i + 1] == b's' {
            if i > 0 && (bytes[i - 1] as char).is_alphanumeric() {
                i += 1;
                continue;
            }
            let mut j = i + 2;
            while j < len && (bytes[j] as char).is_ascii_digit() {
                j += 1;
            }
            if j > i + 2 {
                found.insert(format!("rs{}", &text[i + 2..j]));
            }
        }
        i += 1;
    }
}

fn genotype_display(genotype: &str) -> String {
    if genotype.len() == 2 {
        format!("{}/{}", &genotype[0..1], &genotype[1..2])
    } else {
        genotype.to_string()
    }
}

fn build_claude_context(
    query: &str,
    evidence_snps: &[EvidenceSnp],
    articles: &[WorkbenchArticle],
) -> String {
    let mut parts = Vec::new();

    parts.push(format!("User query: {}", query));
    parts.push(String::new());

    if !evidence_snps.is_empty() {
        parts.push("Relevant variants found in this genome:".to_string());
        for snp in evidence_snps {
            let gene_str = snp.gene.as_deref().unwrap_or("unknown");
            let gt = genotype_display(&snp.genotype);
            parts.push(format!(
                "- {} ({}, chr{}:{}) — Genotype: {}",
                snp.rsid, gene_str, snp.chromosome, snp.position, gt
            ));

            if let Some(ref clinvar) = snp.clinvar {
                parts.push(format!(
                    "  ClinVar: {} — {}",
                    clinvar.clinical_significance, clinvar.condition
                ));
            }

            if !snp.gwas.is_empty() {
                let trait_count = snp.gwas.len();
                let first_trait = &snp.gwas[0];
                let mut gwas_str = format!(
                    "  GWAS: {} studies link to {}",
                    trait_count, first_trait.trait_name
                );
                if let Some(p) = first_trait.p_value {
                    gwas_str.push_str(&format!(" (p={:.2e}", p));
                    if let Some(or) = first_trait.odds_ratio {
                        gwas_str.push_str(&format!(", OR={:.1}", or));
                    }
                    gwas_str.push(')');
                } else if let Some(or) = first_trait.odds_ratio {
                    gwas_str.push_str(&format!(" (OR={:.1})", or));
                }
                parts.push(gwas_str);
            }

            if let Some(ref snpedia) = snp.snpedia {
                let mut snpedia_str = format!(
                    "  SNPedia: {} genotype {}",
                    gt, snpedia.summary
                );
                if let Some(mag) = snpedia.magnitude {
                    snpedia_str.push_str(&format!(" (magnitude: {:.1}", mag));
                    if let Some(ref repute) = snpedia.repute {
                        snpedia_str.push_str(&format!(", {}", repute));
                    }
                    snpedia_str.push(')');
                }
                parts.push(snpedia_str);
            }
        }
    }

    if !articles.is_empty() {
        parts.push(String::new());
        parts.push("Related research articles:".to_string());
        for article in articles {
            let author_str = if article.authors.is_empty() {
                String::new()
            } else {
                format!(" ({}", article.authors)
            };
            let journal_str = if article.journal.is_empty() {
                String::new()
            } else if author_str.is_empty() {
                format!(" ({})", article.journal)
            } else {
                format!(", {})", article.journal)
            };
            let date_str = if article.published_date.is_empty() {
                String::new()
            } else {
                format!(" {}", article.published_date)
            };
            parts.push(format!(
                "- \"{}\"{}{}{}",
                article.title, author_str, journal_str, date_str
            ));
        }
    }

    parts.join("\n")
}
