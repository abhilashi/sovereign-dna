use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// A SNP that contributes to a health risk assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributingSnp {
    pub rsid: String,
    pub gene: String,
    pub genotype: String,
    pub effect: String,
    pub risk_allele: String,
}

/// Health risk analysis result for a single condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthRiskResult {
    pub category: String,
    pub condition: String,
    pub risk_level: String,
    pub score: f64,
    pub contributing_snps: Vec<ContributingSnp>,
    pub study_count: u32,
    pub confidence: String,
    pub source: String,
}

/// Known health risk SNP associations for analysis.
struct RiskSnpDef {
    rsid: &'static str,
    gene: &'static str,
    category: &'static str,
    condition: &'static str,
    risk_allele: &'static str,
    weight: f64,
    effect: &'static str,
}

const RISK_SNP_DEFS: &[RiskSnpDef] = &[
    // Cardiovascular
    RiskSnpDef { rsid: "rs1333049", gene: "CDKN2B-AS1", category: "cardiovascular", condition: "Coronary Artery Disease", risk_allele: "C", weight: 0.3, effect: "Increased risk of coronary artery disease" },
    RiskSnpDef { rsid: "rs10757274", gene: "CDKN2B-AS1", category: "cardiovascular", condition: "Coronary Artery Disease", risk_allele: "G", weight: 0.25, effect: "Associated with myocardial infarction risk" },
    RiskSnpDef { rsid: "rs4420638", gene: "APOC1", category: "cardiovascular", condition: "Coronary Artery Disease", risk_allele: "G", weight: 0.2, effect: "Affects lipid metabolism" },
    RiskSnpDef { rsid: "rs6025", gene: "F5", category: "cardiovascular", condition: "Venous Thromboembolism", risk_allele: "A", weight: 0.7, effect: "Factor V Leiden mutation, increased clotting risk" },
    RiskSnpDef { rsid: "rs1799963", gene: "F2", category: "cardiovascular", condition: "Venous Thromboembolism", risk_allele: "A", weight: 0.5, effect: "Prothrombin G20210A mutation" },
    RiskSnpDef { rsid: "rs1801133", gene: "MTHFR", category: "cardiovascular", condition: "Hyperhomocysteinemia", risk_allele: "T", weight: 0.3, effect: "C677T variant, reduced folate metabolism" },
    RiskSnpDef { rsid: "rs5186", gene: "AGTR1", category: "cardiovascular", condition: "Hypertension", risk_allele: "C", weight: 0.2, effect: "Angiotensin II receptor variant" },

    // Neurological
    RiskSnpDef { rsid: "rs429358", gene: "APOE", category: "neurological", condition: "Alzheimer's Disease", risk_allele: "C", weight: 0.6, effect: "APOE ε4 allele component" },
    RiskSnpDef { rsid: "rs7412", gene: "APOE", category: "neurological", condition: "Alzheimer's Disease", risk_allele: "C", weight: 0.4, effect: "APOE ε2/ε3/ε4 determination" },
    RiskSnpDef { rsid: "rs6265", gene: "BDNF", category: "neurological", condition: "Depression Risk", risk_allele: "T", weight: 0.2, effect: "Val66Met variant affects BDNF secretion" },
    RiskSnpDef { rsid: "rs1800497", gene: "ANKK1/DRD2", category: "neurological", condition: "Dopamine Regulation", risk_allele: "T", weight: 0.25, effect: "Taq1A affects dopamine receptor density" },

    // Metabolic
    RiskSnpDef { rsid: "rs7903146", gene: "TCF7L2", category: "metabolic", condition: "Type 2 Diabetes", risk_allele: "T", weight: 0.35, effect: "Strongest common genetic risk factor for T2D" },
    RiskSnpDef { rsid: "rs1801282", gene: "PPARG", category: "metabolic", condition: "Type 2 Diabetes", risk_allele: "C", weight: 0.2, effect: "Pro12Ala variant affects insulin sensitivity" },
    RiskSnpDef { rsid: "rs9939609", gene: "FTO", category: "metabolic", condition: "Obesity Risk", risk_allele: "A", weight: 0.3, effect: "FTO variant associated with increased BMI" },
    RiskSnpDef { rsid: "rs1260326", gene: "GCKR", category: "metabolic", condition: "Metabolic Syndrome", risk_allele: "T", weight: 0.15, effect: "Affects glucokinase regulation" },

    // Cancer
    RiskSnpDef { rsid: "rs1799950", gene: "BRCA1", category: "cancer", condition: "Breast Cancer", risk_allele: "A", weight: 0.4, effect: "BRCA1 variant associated with cancer risk" },
    RiskSnpDef { rsid: "rs1799966", gene: "BRCA1", category: "cancer", condition: "Breast Cancer", risk_allele: "T", weight: 0.35, effect: "BRCA1 coding variant" },
    RiskSnpDef { rsid: "rs16942", gene: "BRCA1", category: "cancer", condition: "Breast Cancer", risk_allele: "T", weight: 0.2, effect: "BRCA1 missense variant" },
    RiskSnpDef { rsid: "rs1447295", gene: "8q24", category: "cancer", condition: "Prostate Cancer", risk_allele: "A", weight: 0.25, effect: "8q24 region variant" },
    RiskSnpDef { rsid: "rs6983267", gene: "8q24", category: "cancer", condition: "Colorectal Cancer", risk_allele: "G", weight: 0.2, effect: "8q24 region risk variant" },

    // Autoimmune
    RiskSnpDef { rsid: "rs2476601", gene: "PTPN22", category: "autoimmune", condition: "Autoimmune Disease Risk", risk_allele: "A", weight: 0.3, effect: "R620W variant affects T-cell signaling" },
    RiskSnpDef { rsid: "rs3184504", gene: "SH2B3", category: "autoimmune", condition: "Celiac Disease", risk_allele: "T", weight: 0.25, effect: "LNK variant associated with celiac risk" },
    RiskSnpDef { rsid: "rs2104286", gene: "IL2RA", category: "autoimmune", condition: "Type 1 Diabetes", risk_allele: "A", weight: 0.2, effect: "IL-2 receptor variant affects immune regulation" },
    RiskSnpDef { rsid: "rs6897932", gene: "IL7R", category: "autoimmune", condition: "Multiple Sclerosis", risk_allele: "C", weight: 0.2, effect: "IL-7 receptor variant" },
];

/// Analyze health risks by joining the user's SNPs with annotations and known risk associations.
pub fn analyze_health_risks(
    conn: &Connection,
    genome_id: i64,
) -> Result<Vec<HealthRiskResult>, AppError> {
    // Collect all risk rsids we want to check
    let risk_rsids: Vec<&str> = RISK_SNP_DEFS.iter().map(|d| d.rsid).collect();

    // Query user's SNPs for all risk-related rsids in one go
    let placeholders: String = risk_rsids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT s.rsid, s.genotype, COALESCE(a.gene, '') as gene
         FROM snps s
         LEFT JOIN annotations a ON s.rsid = a.rsid
         WHERE s.genome_id = ?1 AND s.rsid IN ({})",
        placeholders
    );

    let mut stmt = conn.prepare(&sql)?;

    // Build params: genome_id + all rsids
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    params.push(Box::new(genome_id));
    for rsid in &risk_rsids {
        params.push(Box::new(rsid.to_string()));
    }

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let user_snps: Vec<(String, String, String)> = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Build a lookup map: rsid -> (genotype, gene)
    let mut snp_map: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    for (rsid, genotype, gene) in user_snps {
        snp_map.insert(rsid, (genotype, gene));
    }

    // Group risk definitions by (category, condition) and compute scores
    let mut condition_groups: std::collections::HashMap<
        (String, String),
        Vec<(ContributingSnp, f64)>,
    > = std::collections::HashMap::new();

    for def in RISK_SNP_DEFS {
        if let Some((genotype, db_gene)) = snp_map.get(def.rsid) {
            let gene_name = if db_gene.is_empty() {
                def.gene.to_string()
            } else {
                db_gene.clone()
            };

            // Count how many copies of the risk allele the user has
            let risk_allele_count = genotype
                .chars()
                .filter(|c| c.to_string().eq_ignore_ascii_case(def.risk_allele))
                .count();

            let allele_weight = match risk_allele_count {
                0 => 0.0,
                1 => 0.5, // heterozygous
                _ => 1.0, // homozygous for risk allele
            };

            let weighted_score = def.weight * allele_weight;

            let contributing = ContributingSnp {
                rsid: def.rsid.to_string(),
                gene: gene_name,
                genotype: genotype.clone(),
                effect: def.effect.to_string(),
                risk_allele: def.risk_allele.to_string(),
            };

            let key = (def.category.to_string(), def.condition.to_string());
            condition_groups
                .entry(key)
                .or_default()
                .push((contributing, weighted_score));
        }
    }

    // Also pull in annotations from the database for enrichment
    let annotation_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM snps s
             INNER JOIN annotations a ON s.rsid = a.rsid
             WHERE s.genome_id = ?1",
            [genome_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let mut results = Vec::new();

    for ((category, condition), snp_scores) in &condition_groups {
        let total_possible: f64 = RISK_SNP_DEFS
            .iter()
            .filter(|d| d.category == category && d.condition == condition)
            .map(|d| d.weight)
            .sum();

        let actual_score: f64 = snp_scores.iter().map(|(_, s)| s).sum();
        let normalized_score = if total_possible > 0.0 {
            (actual_score / total_possible).min(1.0)
        } else {
            0.0
        };

        let risk_level = if normalized_score < 0.2 {
            "low"
        } else if normalized_score < 0.4 {
            "moderate"
        } else if normalized_score < 0.7 {
            "elevated"
        } else {
            "high"
        };

        let confidence = if snp_scores.len() >= 3 {
            "high"
        } else if snp_scores.len() >= 2 {
            "moderate"
        } else {
            "low"
        };

        let study_count = if annotation_count > 0 {
            (snp_scores.len() as u32) * 3
        } else {
            snp_scores.len() as u32
        };

        results.push(HealthRiskResult {
            category: category.clone(),
            condition: condition.clone(),
            risk_level: risk_level.to_string(),
            score: (normalized_score * 100.0).round() / 100.0,
            contributing_snps: snp_scores.iter().map(|(s, _)| s.clone()).collect(),
            study_count,
            confidence: confidence.to_string(),
            source: "curated".to_string(),
        });
    }

    // Collect conditions already covered by curated results for deduplication
    let curated_conditions: std::collections::HashSet<String> = results
        .iter()
        .map(|r| r.condition.to_lowercase())
        .collect();

    // --- ClinVar enrichment ---
    if is_reference_ready(conn, "clinvar") {
        let mut clinvar_stmt = conn.prepare(
            "SELECT s.rsid, s.genotype, a.gene, a.clinical_significance, a.condition, a.review_status
             FROM snps s
             INNER JOIN annotations a ON s.rsid = a.rsid
             WHERE s.genome_id = ?1
               AND (a.clinical_significance LIKE '%pathogenic%'
                    OR a.clinical_significance LIKE '%risk%')"
        )?;

        let clinvar_rows: Vec<(String, String, String, String, String, Option<String>)> = clinvar_stmt
            .query_map([genome_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2).unwrap_or_default(),
                    row.get::<_, String>(3).unwrap_or_default(),
                    row.get::<_, String>(4).unwrap_or_default(),
                    row.get::<_, Option<String>>(5)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        // Group by condition
        let mut clinvar_groups: std::collections::HashMap<
            String,
            Vec<(String, String, String, String, Option<String>)>,
        > = std::collections::HashMap::new();
        for (rsid, genotype, gene, clin_sig, condition, review_status) in clinvar_rows {
            if condition.is_empty() {
                continue;
            }
            clinvar_groups
                .entry(condition.clone())
                .or_default()
                .push((rsid, genotype, gene, clin_sig, review_status));
        }

        for (condition, variants) in clinvar_groups {
            if curated_conditions.contains(&condition.to_lowercase()) {
                continue;
            }

            // Determine risk_level from the most severe clinical significance
            let mut risk_level = "low".to_string();
            for (_, _, _, clin_sig, _) in &variants {
                let sig_lower = clin_sig.to_lowercase();
                if sig_lower.contains("pathogenic") && !sig_lower.contains("likely") {
                    risk_level = "elevated".to_string();
                    break;
                } else if sig_lower.contains("likely pathogenic") {
                    risk_level = "moderate".to_string();
                }
            }

            // Determine confidence from review_status
            let confidence = variants
                .iter()
                .find_map(|(_, _, _, _, rs)| rs.as_ref())
                .map(|rs| {
                    if rs.contains("practice guideline") || rs.contains("reviewed by expert") {
                        "high"
                    } else if rs.contains("criteria provided") {
                        "moderate"
                    } else {
                        "low"
                    }
                })
                .unwrap_or("low")
                .to_string();

            let contributing_snps: Vec<ContributingSnp> = variants
                .iter()
                .map(|(rsid, genotype, gene, clin_sig, _)| ContributingSnp {
                    rsid: rsid.clone(),
                    gene: gene.clone(),
                    genotype: genotype.clone(),
                    effect: clin_sig.clone(),
                    risk_allele: String::new(),
                })
                .collect();

            let study_count = contributing_snps.len() as u32;

            results.push(HealthRiskResult {
                category: "clinvar".to_string(),
                condition,
                risk_level,
                score: 0.0,
                contributing_snps,
                study_count,
                confidence,
                source: "clinvar".to_string(),
            });
        }
    }

    // --- GWAS Catalog enrichment ---
    if is_reference_ready(conn, "gwas_catalog") {
        let mut gwas_stmt = conn.prepare(
            "SELECT g.rsid, g.trait_name, g.p_value, g.odds_ratio, g.risk_allele, s.genotype
             FROM gwas_associations g
             INNER JOIN snps s ON g.rsid = s.rsid
             WHERE s.genome_id = ?1 AND g.p_value < 5e-8
             ORDER BY g.p_value"
        )?;

        let gwas_rows: Vec<(String, String, f64, Option<f64>, Option<String>, String)> = gwas_stmt
            .query_map([genome_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        // Group by trait_name
        let mut gwas_groups: std::collections::HashMap<
            String,
            Vec<(String, f64, Option<f64>, Option<String>, String)>,
        > = std::collections::HashMap::new();
        for (rsid, trait_name, p_value, odds_ratio, risk_allele, genotype) in gwas_rows {
            gwas_groups
                .entry(trait_name)
                .or_default()
                .push((rsid, p_value, odds_ratio, risk_allele, genotype));
        }

        // Collect conditions from curated + clinvar for dedup
        let all_conditions: std::collections::HashSet<String> = results
            .iter()
            .map(|r| r.condition.to_lowercase())
            .collect();

        for (trait_name, associations) in gwas_groups {
            if all_conditions.contains(&trait_name.to_lowercase()) {
                continue;
            }

            // Determine risk_level from the max odds_ratio
            let max_or = associations
                .iter()
                .filter_map(|(_, _, or, _, _)| *or)
                .fold(1.0_f64, f64::max);

            let risk_level = if max_or > 2.0 {
                "high"
            } else if max_or > 1.5 {
                "elevated"
            } else if max_or > 1.2 {
                "moderate"
            } else {
                "low"
            };

            let contributing_snps: Vec<ContributingSnp> = associations
                .iter()
                .map(|(rsid, p_value, odds_ratio, risk_allele, genotype)| ContributingSnp {
                    rsid: rsid.clone(),
                    gene: String::new(),
                    genotype: genotype.clone(),
                    effect: format!(
                        "p={:.2e}{}",
                        p_value,
                        odds_ratio.map(|or| format!(", OR={:.2}", or)).unwrap_or_default()
                    ),
                    risk_allele: risk_allele.clone().unwrap_or_default(),
                })
                .collect();

            let study_count = contributing_snps.len() as u32;

            results.push(HealthRiskResult {
                category: "gwas".to_string(),
                condition: trait_name,
                risk_level: risk_level.to_string(),
                score: 0.0,
                contributing_snps,
                study_count,
                confidence: "moderate".to_string(),
                source: "gwas_catalog".to_string(),
            });
        }
    }

    // Sort by score descending
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    Ok(results)
}

/// Check if a reference database is downloaded and ready.
fn is_reference_ready(conn: &Connection, source: &str) -> bool {
    conn.query_row(
        "SELECT status FROM reference_status WHERE source = ?1",
        [source],
        |row| row.get::<_, String>(0),
    )
    .map(|status| status == "ready")
    .unwrap_or(false)
}
