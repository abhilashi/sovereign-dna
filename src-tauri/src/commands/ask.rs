use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

use crate::db::Database;
use crate::error::AppError;
use crate::research::intent::{parse_question, QueryIntent};

// ── Response Structures ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenomeAnswer {
    pub question: String,
    pub answer: String,
    pub sources: Vec<AnswerSource>,
    pub related_snps: Vec<RelatedSnp>,
    pub confidence: String,
    pub disclaimer: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSource {
    pub source_type: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedSnp {
    pub rsid: String,
    pub chromosome: String,
    pub position: i64,
    pub genotype: String,
    pub gene: Option<String>,
    pub significance: Option<String>,
}

// ── Constants ───────────────────────────────────────────────────

const MEDICAL_DISCLAIMER: &str = "This information is for educational and informational purposes only. It is not intended as medical advice and should not be used to make health decisions. Genetic risk factors represent statistical associations and do not determine outcomes. Always consult a qualified healthcare provider or genetic counselor for interpretation of genetic results.";

// ── Helper functions ────────────────────────────────────────────

fn format_position(pos: i64) -> String {
    let s = pos.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

fn genotype_display(genotype: &str) -> String {
    if genotype.len() == 2 {
        format!("{}/{}", &genotype[0..1], &genotype[1..2])
    } else {
        genotype.to_string()
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

/// Helper to query 7-column rows from snps+annotations
fn query_annotated_snps(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::types::ToSql],
) -> Result<Vec<(String, String, i64, String, Option<String>, Option<String>, Option<String>)>, AppError> {
    let mut stmt = conn.prepare(sql)?;
    let results = stmt
        .query_map(params, |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(results)
}

/// Helper to query 6-column rows (rsid, chr, pos, genotype, significance, condition)
fn query_gene_variants(
    conn: &Connection,
    genome_id: i64,
    gene: &str,
) -> Result<Vec<(String, String, i64, String, Option<String>, Option<String>)>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT s.rsid, s.chromosome, s.position, s.genotype,
                a.clinical_significance, a.condition
         FROM snps s
         INNER JOIN annotations a ON s.rsid = a.rsid
         WHERE s.genome_id = ?1 AND UPPER(a.gene) = UPPER(?2)
         ORDER BY s.position",
    )?;
    let results = stmt
        .query_map(rusqlite::params![genome_id, gene], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(results)
}

// ── Answer Builders ─────────────────────────────────────────────

pub(crate) fn build_rsid_answer(conn: &Connection, genome_id: i64, rsid: &str) -> Result<GenomeAnswer, AppError> {
    let mut sources = Vec::new();
    let mut related_snps = Vec::new();
    let mut answer_parts: Vec<String> = Vec::new();
    let mut confidence = "moderate";

    // 1. Get the SNP from user's genome
    let snp_result = conn.query_row(
        "SELECT rsid, chromosome, position, genotype FROM snps WHERE genome_id = ?1 AND rsid = ?2",
        rusqlite::params![genome_id, rsid],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    );

    let (rsid_val, chromosome, position, genotype) = match snp_result {
        Ok(data) => data,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Ok(GenomeAnswer {
                question: format!("What is {}?", rsid),
                answer: format!(
                    "**{} was not found in your genome data.**\n\nThis variant is not present in your imported genome file. This could mean:\n\n- Your genotyping platform did not test this position\n- The variant was filtered out during import\n- This rsID may not be valid\n\nTry searching for this rsID in the SNP Explorer for more details.",
                    rsid
                ),
                sources: vec![AnswerSource {
                    source_type: "your_genome".to_string(),
                    detail: "Variant not found in imported data".to_string(),
                }],
                related_snps: vec![],
                confidence: "high".to_string(),
                disclaimer: MEDICAL_DISCLAIMER.to_string(),
            });
        }
        Err(e) => return Err(AppError::Database(e.to_string())),
    };

    let gt = genotype_display(&genotype);

    answer_parts.push(format!("**Your genotype at {} is {}**", rsid_val, gt));

    sources.push(AnswerSource {
        source_type: "your_genome".to_string(),
        detail: format!("Genotype: {}", gt),
    });

    // 2. Get annotations
    let mut ann_stmt = conn.prepare(
        "SELECT gene, clinical_significance, condition, source
         FROM annotations WHERE rsid = ?1",
    )?;
    let ann_result: Vec<(Option<String>, Option<String>, Option<String>, Option<String>)> = ann_stmt
        .query_map([&rsid_val], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let mut gene_name: Option<String> = None;

    if !ann_result.is_empty() {
        for (gene, significance, condition, source) in &ann_result {
            if gene.is_some() && gene_name.is_none() {
                gene_name = gene.clone();
            }

            let mut detail_parts = Vec::new();
            if let Some(g) = gene {
                detail_parts.push(format!(
                    "This variant is in the *{}* gene on chromosome {} (position {}).",
                    g,
                    chromosome,
                    format_position(position)
                ));
            } else {
                detail_parts.push(format!(
                    "This variant is on chromosome {} (position {}).",
                    chromosome,
                    format_position(position)
                ));
            }

            if let Some(sig) = significance {
                detail_parts.push(format!("\n\n**Clinical Significance:** {}", sig));
                if sig.to_lowercase().contains("pathogenic") {
                    confidence = "high";
                }
            }

            if let Some(cond) = condition {
                detail_parts.push(format!("\n\n**Associated Condition:** {}", cond));
            }

            answer_parts.push(detail_parts.join(""));

            if let Some(src) = source {
                sources.push(AnswerSource {
                    source_type: "clinvar".to_string(),
                    detail: format!("Source: {}", src),
                });
            }
        }
    } else {
        answer_parts.push(format!(
            "This variant is on chromosome {} (position {}). No clinical annotations are currently available in your local database.",
            chromosome,
            format_position(position)
        ));
    }

    // 3. Get GWAS associations
    let mut gwas_stmt = conn.prepare(
        "SELECT trait_name, odds_ratio, risk_allele FROM gwas_associations WHERE rsid = ?1",
    )?;
    let gwas_results: Vec<(String, Option<f64>, Option<String>)> = gwas_stmt
        .query_map([&rsid_val], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<f64>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    if !gwas_results.is_empty() {
        let trait_count = gwas_results.len();
        let traits: Vec<&str> = gwas_results
            .iter()
            .take(5)
            .map(|(t, _, _)| t.as_str())
            .collect();
        let traits_str = traits.join(", ");

        if trait_count > 5 {
            answer_parts.push(format!(
                "\n\n**GWAS Studies:** {} genome-wide association studies have linked this variant to traits including: {}... and {} more.",
                trait_count, traits_str, trait_count - 5
            ));
        } else {
            answer_parts.push(format!(
                "\n\n**GWAS Studies:** {} genome-wide association {} linked this variant to: {}.",
                trait_count,
                if trait_count == 1 {
                    "study has"
                } else {
                    "studies have"
                },
                traits_str
            ));
        }

        sources.push(AnswerSource {
            source_type: "gwas".to_string(),
            detail: format!("{} associated traits", trait_count),
        });
        confidence = "high";
    }

    // 4. Get SNPedia entries for user's genotype
    let mut snpedia_stmt = conn.prepare(
        "SELECT magnitude, repute, summary FROM snpedia_entries WHERE rsid = ?1 AND genotype = ?2",
    )?;
    let snpedia_results: Vec<(Option<f64>, Option<String>, Option<String>)> = snpedia_stmt
        .query_map(rusqlite::params![rsid_val, genotype], |row| {
            Ok((
                row.get::<_, Option<f64>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    if !snpedia_results.is_empty() {
        for (magnitude, _repute, summary) in &snpedia_results {
            if let Some(summ) = summary {
                let mag_str = magnitude
                    .map(|m| format!(" (magnitude: {:.1})", m))
                    .unwrap_or_default();
                answer_parts.push(format!("\n\n**SNPedia:** {}{}", summ, mag_str));
            }
        }
        sources.push(AnswerSource {
            source_type: "snpedia".to_string(),
            detail: "Community-curated variant information".to_string(),
        });
    }

    related_snps.push(RelatedSnp {
        rsid: rsid_val.clone(),
        chromosome,
        position,
        genotype: gt,
        gene: gene_name,
        significance: ann_result.first().and_then(|(_, s, _, _)| s.clone()),
    });

    Ok(GenomeAnswer {
        question: format!("What is {}?", rsid),
        answer: answer_parts.join("\n\n"),
        sources,
        related_snps,
        confidence: confidence.to_string(),
        disclaimer: MEDICAL_DISCLAIMER.to_string(),
    })
}

pub(crate) fn build_gene_answer(
    conn: &Connection,
    genome_id: i64,
    gene: &str,
) -> Result<GenomeAnswer, AppError> {
    let mut sources = Vec::new();
    let mut related_snps = Vec::new();

    let variants = query_gene_variants(conn, genome_id, gene)?;

    if variants.is_empty() {
        // Check GWAS for gene mentions
        let mut gwas_stmt = conn.prepare(
            "SELECT DISTINCT g.rsid, g.trait_name
             FROM gwas_associations g
             INNER JOIN snps s ON g.rsid = s.rsid
             WHERE s.genome_id = ?1 AND g.trait_name LIKE ?2
             LIMIT 10",
        )?;
        let pattern = format!("%{}%", gene);
        let gwas_gene_hits: Vec<(String, String)> = gwas_stmt
            .query_map(rusqlite::params![genome_id, pattern], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        if gwas_gene_hits.is_empty() {
            return Ok(GenomeAnswer {
                question: format!("What about {}?", gene),
                answer: format!(
                    "**No annotated variants found in {}**\n\nYour genome data does not contain any variants in the *{}* gene that have been annotated in your local reference databases. This could mean:\n\n- Your genotyping platform did not cover variants in this gene\n- No clinically significant variants were found\n- Reference data for this gene has not been downloaded yet\n\nTry downloading ClinVar reference data from the Settings page to get more annotations.",
                    gene, gene
                ),
                sources: vec![AnswerSource {
                    source_type: "your_genome".to_string(),
                    detail: format!("No annotated variants in {}", gene),
                }],
                related_snps: vec![],
                confidence: "moderate".to_string(),
                disclaimer: MEDICAL_DISCLAIMER.to_string(),
            });
        }
    }

    let mut answer_parts = Vec::new();
    answer_parts.push(format!(
        "**You have {} annotated variant{} in *{}***",
        variants.len(),
        if variants.len() == 1 { "" } else { "s" },
        gene
    ));

    sources.push(AnswerSource {
        source_type: "your_genome".to_string(),
        detail: format!("{} variants in {}", variants.len(), gene),
    });

    let mut has_pathogenic = false;
    let mut has_clinvar = false;

    for (rsid, chromosome, position, genotype, significance, condition) in &variants {
        let gt = genotype_display(genotype);

        let mut detail = format!("- **{}** ({})", rsid, gt);

        if let Some(sig) = significance {
            detail.push_str(&format!(" — {}", sig));
            has_clinvar = true;
            if sig.to_lowercase().contains("pathogenic") {
                has_pathogenic = true;
            }
        }
        if let Some(cond) = condition {
            detail.push_str(&format!(" ({})", cond));
        }

        answer_parts.push(detail);

        related_snps.push(RelatedSnp {
            rsid: rsid.clone(),
            chromosome: chromosome.clone(),
            position: *position,
            genotype: gt,
            gene: Some(gene.to_string()),
            significance: significance.clone(),
        });
    }

    if has_clinvar {
        sources.push(AnswerSource {
            source_type: "clinvar".to_string(),
            detail: "Clinical variant annotations".to_string(),
        });
    }

    // Check GWAS associations for this gene's SNPs
    let rsids: Vec<String> = related_snps.iter().map(|s| s.rsid.clone()).collect();
    if !rsids.is_empty() {
        let placeholders: Vec<String> = (1..=rsids.len()).map(|i| format!("?{}", i)).collect();
        let gwas_sql = format!(
            "SELECT COUNT(DISTINCT trait_name) FROM gwas_associations WHERE rsid IN ({})",
            placeholders.join(", ")
        );
        let mut stmt = conn.prepare(&gwas_sql)?;
        let params: Vec<&dyn rusqlite::types::ToSql> =
            rsids.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
        let gwas_count: i64 = stmt
            .query_row(params.as_slice(), |row| row.get(0))
            .unwrap_or(0);

        if gwas_count > 0 {
            answer_parts.push(format!(
                "\n**GWAS associations:** {} related trait{} found in genome-wide association studies.",
                gwas_count,
                if gwas_count == 1 { "" } else { "s" }
            ));
            sources.push(AnswerSource {
                source_type: "gwas".to_string(),
                detail: format!("{} trait associations", gwas_count),
            });
        }
    }

    let confidence = if has_pathogenic {
        "high"
    } else if has_clinvar {
        "moderate"
    } else {
        "low"
    };

    Ok(GenomeAnswer {
        question: format!("What about {}?", gene),
        answer: answer_parts.join("\n"),
        sources,
        related_snps,
        confidence: confidence.to_string(),
        disclaimer: MEDICAL_DISCLAIMER.to_string(),
    })
}

pub(crate) fn build_condition_risk_answer(
    conn: &Connection,
    genome_id: i64,
    condition: &str,
) -> Result<GenomeAnswer, AppError> {
    let mut sources = Vec::new();
    let mut related_snps = Vec::new();
    let mut answer_parts = Vec::new();

    let is_all = condition == "all";

    // Query annotations matching the condition
    let condition_variants = if is_all {
        query_annotated_snps(
            conn,
            "SELECT s.rsid, s.chromosome, s.position, s.genotype,
                    a.gene, a.clinical_significance, a.condition
             FROM snps s
             INNER JOIN annotations a ON s.rsid = a.rsid
             WHERE s.genome_id = ?1
             AND a.clinical_significance IS NOT NULL
             ORDER BY a.clinical_significance, s.chromosome
             LIMIT 50",
            &[&genome_id as &dyn rusqlite::types::ToSql],
        )?
    } else {
        let pattern = format!("%{}%", condition.to_lowercase());
        query_annotated_snps(
            conn,
            "SELECT s.rsid, s.chromosome, s.position, s.genotype,
                    a.gene, a.clinical_significance, a.condition
             FROM snps s
             INNER JOIN annotations a ON s.rsid = a.rsid
             WHERE s.genome_id = ?1
             AND (LOWER(a.condition) LIKE ?2 OR LOWER(a.gene) LIKE ?2)
             ORDER BY a.clinical_significance
             LIMIT 30",
            &[
                &genome_id as &dyn rusqlite::types::ToSql,
                &pattern as &dyn rusqlite::types::ToSql,
            ],
        )?
    };

    // Also check GWAS associations
    let gwas_hits: Vec<(String, String, Option<f64>, Option<String>, String, String, i64)> = {
        let gwas_sql = if is_all {
            "SELECT g.rsid, g.trait_name, g.odds_ratio, g.risk_allele, s.genotype, s.chromosome, s.position
             FROM gwas_associations g
             INNER JOIN snps s ON g.rsid = s.rsid
             WHERE s.genome_id = ?1
             ORDER BY g.odds_ratio DESC NULLS LAST
             LIMIT 30"
        } else {
            "SELECT g.rsid, g.trait_name, g.odds_ratio, g.risk_allele, s.genotype, s.chromosome, s.position
             FROM gwas_associations g
             INNER JOIN snps s ON g.rsid = s.rsid
             WHERE s.genome_id = ?1 AND LOWER(g.trait_name) LIKE ?2
             ORDER BY g.odds_ratio DESC NULLS LAST
             LIMIT 20"
        };
        let pattern = format!("%{}%", condition.to_lowercase());
        let mut stmt = conn.prepare(gwas_sql)?;
        let map_fn = |row: &rusqlite::Row<'_>| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<f64>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
            ))
        };
        if is_all {
            stmt.query_map(rusqlite::params![genome_id], map_fn)?
                .filter_map(|r| r.ok())
                .collect()
        } else {
            stmt.query_map(rusqlite::params![genome_id, pattern], map_fn)?
                .filter_map(|r| r.ok())
                .collect()
        }
    };

    let display_condition = if is_all { "health risks" } else { condition };

    if condition_variants.is_empty() && gwas_hits.is_empty() {
        return Ok(GenomeAnswer {
            question: format!("Am I at risk for {}?", display_condition),
            answer: format!(
                "**No specific variants found related to {}**\n\nYour genome data does not contain annotated variants directly linked to {} in your current reference databases. This does not mean you have zero risk — it means no relevant variants were identified with current data.\n\nTo get more comprehensive results, try downloading reference databases (ClinVar, GWAS Catalog) from the Settings page.",
                display_condition, display_condition
            ),
            sources: vec![AnswerSource {
                source_type: "your_genome".to_string(),
                detail: "No annotated risk variants found".to_string(),
            }],
            related_snps: vec![],
            confidence: "low".to_string(),
            disclaimer: MEDICAL_DISCLAIMER.to_string(),
        });
    }

    answer_parts.push(format!("**Regarding {}:**", display_condition));

    if !condition_variants.is_empty() {
        answer_parts.push(format!(
            "\n**Clinical annotations:** {} relevant variant{} found.",
            condition_variants.len(),
            if condition_variants.len() == 1 { "" } else { "s" }
        ));

        for (rsid, chr, pos, genotype, gene, significance, cond) in &condition_variants {
            let gt = genotype_display(genotype);

            let mut detail = format!("- **{}** ({})", rsid, gt);
            if let Some(g) = gene {
                detail.push_str(&format!(" in *{}*", g));
            }
            if let Some(sig) = significance {
                detail.push_str(&format!(" — {}", sig));
            }
            if let Some(c) = cond {
                if !is_all {
                    detail.push_str(&format!(" ({})", c));
                }
            }
            answer_parts.push(detail);

            related_snps.push(RelatedSnp {
                rsid: rsid.clone(),
                chromosome: chr.clone(),
                position: *pos,
                genotype: gt,
                gene: gene.clone(),
                significance: significance.clone(),
            });
        }

        sources.push(AnswerSource {
            source_type: "clinvar".to_string(),
            detail: format!("{} annotated variants", condition_variants.len()),
        });
    }

    if !gwas_hits.is_empty() {
        answer_parts.push(format!(
            "\n**GWAS associations:** {} variant{} linked in genome-wide association studies.",
            gwas_hits.len(),
            if gwas_hits.len() == 1 { "" } else { "s" }
        ));

        for (rsid, trait_name, odds_ratio, risk_allele, genotype, chr, pos) in
            gwas_hits.iter().take(10)
        {
            let gt = genotype_display(genotype);

            let mut detail = format!("- **{}** ({}) — {}", rsid, gt, trait_name);
            if let Some(or_val) = odds_ratio {
                detail.push_str(&format!(" (OR: {:.2})", or_val));
            }
            if let Some(ra) = risk_allele {
                if let Some(risk_char) = ra.chars().next() {
                    let has_risk = genotype.contains(risk_char);
                    if has_risk {
                        detail.push_str(" — *you carry the risk allele*");
                    }
                }
            }
            answer_parts.push(detail);

            if !related_snps.iter().any(|s| s.rsid == *rsid) {
                related_snps.push(RelatedSnp {
                    rsid: rsid.clone(),
                    chromosome: chr.clone(),
                    position: *pos,
                    genotype: gt,
                    gene: None,
                    significance: Some(trait_name.clone()),
                });
            }
        }

        sources.push(AnswerSource {
            source_type: "gwas".to_string(),
            detail: format!("{} GWAS associations", gwas_hits.len()),
        });
    }

    let total_variants = condition_variants.len() + gwas_hits.len();
    answer_parts.push(format!(
        "\nBased on {} variant{} analyzed.",
        total_variants,
        if total_variants == 1 { "" } else { "s" }
    ));

    sources.push(AnswerSource {
        source_type: "your_genome".to_string(),
        detail: "Your genomic data".to_string(),
    });

    let confidence = if condition_variants.len() > 3 || gwas_hits.len() > 5 {
        "moderate"
    } else {
        "low"
    };

    Ok(GenomeAnswer {
        question: format!("Am I at risk for {}?", display_condition),
        answer: answer_parts.join("\n"),
        sources,
        related_snps,
        confidence: confidence.to_string(),
        disclaimer: MEDICAL_DISCLAIMER.to_string(),
    })
}

pub(crate) fn build_drug_response_answer(
    conn: &Connection,
    genome_id: i64,
    drug: &str,
) -> Result<GenomeAnswer, AppError> {
    let mut sources = Vec::new();
    let mut related_snps = Vec::new();
    let mut answer_parts = Vec::new();

    let is_all = drug == "all";

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
    let relevant_genes: Vec<&str> = if is_all {
        pharma_genes.iter().map(|(g, _)| *g).collect()
    } else {
        pharma_genes
            .iter()
            .filter(|(_, drugs)| drugs.iter().any(|d| *d == drug_lower.as_str()))
            .map(|(g, _)| *g)
            .collect()
    };

    let display_drug = if is_all { "drug metabolism" } else { drug };

    answer_parts.push(format!("**Regarding {}:**", display_drug));

    let mut total_found = 0;

    for gene in &relevant_genes {
        let variants = query_gene_variants(conn, genome_id, gene)?;

        if !variants.is_empty() {
            total_found += variants.len();
            answer_parts.push(format!(
                "\n**{}** — {} variant{} found:",
                gene,
                variants.len(),
                if variants.len() == 1 { "" } else { "s" }
            ));

            for (rsid, chr, pos, genotype, significance, _condition) in &variants {
                let gt = genotype_display(genotype);

                let mut detail = format!("- **{}** ({})", rsid, gt);
                if let Some(sig) = significance {
                    detail.push_str(&format!(" — {}", sig));
                }
                answer_parts.push(detail);

                related_snps.push(RelatedSnp {
                    rsid: rsid.clone(),
                    chromosome: chr.clone(),
                    position: *pos,
                    genotype: gt,
                    gene: Some(gene.to_string()),
                    significance: significance.clone(),
                });
            }
        }
    }

    if total_found == 0 {
        answer_parts.push(format!(
            "\nNo pharmacogenomic variants were found in your genome data for {}. This could mean:\n\n- Your genotyping platform does not cover these pharmacogenomic markers\n- No variants were detected in the relevant genes ({})\n- Reference data for pharmacogenomics has not been downloaded\n\nCheck the Pharmacogenomics page for a full analysis, or download ClinVar reference data from Settings.",
            display_drug,
            relevant_genes.join(", ")
        ));
    } else {
        answer_parts.push(format!(
            "\n{} pharmacogenomic variant{} found. Visit the Pharmacogenomics page for a detailed analysis including metabolizer status predictions.",
            total_found,
            if total_found == 1 { "" } else { "s" }
        ));
    }

    sources.push(AnswerSource {
        source_type: "your_genome".to_string(),
        detail: "Your genomic data".to_string(),
    });
    if total_found > 0 {
        sources.push(AnswerSource {
            source_type: "clinvar".to_string(),
            detail: "Clinical variant annotations".to_string(),
        });
    }

    let confidence = if total_found > 2 { "moderate" } else { "low" };

    Ok(GenomeAnswer {
        question: format!("How do I respond to {}?", display_drug),
        answer: answer_parts.join("\n"),
        sources,
        related_snps,
        confidence: confidence.to_string(),
        disclaimer: MEDICAL_DISCLAIMER.to_string(),
    })
}

pub(crate) fn build_trait_answer(
    conn: &Connection,
    genome_id: i64,
    trait_name: &str,
) -> Result<GenomeAnswer, AppError> {
    let mut sources = Vec::new();
    let mut related_snps = Vec::new();
    let mut answer_parts = Vec::new();

    // Known trait-SNP associations
    let trait_snps: &[(&str, &str, &str)] = match trait_name {
        "eye color" => &[
            ("rs12913832", "HERC2", "Primary eye color determinant; GG=blue, AG=green/hazel, AA=brown"),
            ("rs1800407", "OCA2", "Modifies eye color; may contribute to green/hazel"),
            ("rs12896399", "SLC24A4", "Associated with eye color variation"),
        ],
        "hair color" => &[
            ("rs12913832", "HERC2", "Also affects hair color pigmentation"),
            ("rs1805007", "MC1R", "Associated with red hair; CC=typical, CT/TT=increased red hair likelihood"),
            ("rs1805008", "MC1R", "Second MC1R variant associated with red hair"),
        ],
        "lactose intolerance" => &[
            ("rs4988235", "MCM6/LCT", "C/T at -13910 near LCT; CC=likely lactose intolerant, CT/TT=likely lactose tolerant"),
        ],
        "muscle composition" => &[
            ("rs1815739", "ACTN3", "R577X; CC=sprint/power advantage, CT=mixed, TT=endurance advantage"),
        ],
        "alcohol flush reaction" => &[
            ("rs671", "ALDH2", "GG=normal, GA/AA=alcohol flush reaction (common in East Asian populations)"),
        ],
        "bitter taste perception" => &[
            ("rs713598", "TAS2R38", "Affects ability to taste bitter compounds like PTC/PROP"),
            ("rs1726866", "TAS2R38", "Second TAS2R38 variant for bitter taste"),
        ],
        "cilantro taste" => &[
            ("rs72921001", "OR6A2", "Associated with perception of cilantro as soapy"),
        ],
        "earwax type" => &[
            ("rs17822931", "ABCC11", "CC/CT=wet earwax, TT=dry earwax"),
        ],
        "caffeine metabolism" => &[
            ("rs762551", "CYP1A2", "AA=fast caffeine metabolizer, AC/CC=slow metabolizer"),
        ],
        _ => &[],
    };

    answer_parts.push(format!("**{}:**", capitalize(trait_name)));

    if trait_snps.is_empty() {
        // Try SNPedia
        let pattern = format!("%{}%", trait_name);
        let mut snpedia_stmt = conn.prepare(
            "SELECT se.rsid, se.genotype, se.magnitude, se.repute, se.summary
             FROM snpedia_entries se
             INNER JOIN snps s ON se.rsid = s.rsid AND se.genotype = s.genotype
             WHERE s.genome_id = ?1 AND LOWER(se.summary) LIKE ?2
             ORDER BY se.magnitude DESC NULLS LAST
             LIMIT 10",
        )?;
        let snpedia_hits: Vec<(String, String, Option<f64>, Option<String>, Option<String>)> =
            snpedia_stmt
                .query_map(rusqlite::params![genome_id, pattern], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<f64>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                })?
                .filter_map(|r| r.ok())
                .collect();

        if snpedia_hits.is_empty() {
            answer_parts.push(format!(
                "\nNo specific variants related to {} were found in your genome data. Check the Traits page for a comprehensive trait analysis.",
                trait_name
            ));
        } else {
            for (rsid, genotype, _mag, _repute, summary) in &snpedia_hits {
                let gt = genotype_display(genotype);
                if let Some(summ) = summary {
                    answer_parts.push(format!("- **{}** ({}) — {}", rsid, gt, summ));
                }
            }
            sources.push(AnswerSource {
                source_type: "snpedia".to_string(),
                detail: "Community-curated trait data".to_string(),
            });
        }
    } else {
        for &(rsid, gene, description) in trait_snps {
            let snp_result = conn.query_row(
                "SELECT genotype, chromosome, position FROM snps WHERE genome_id = ?1 AND rsid = ?2",
                rusqlite::params![genome_id, rsid],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            );

            match snp_result {
                Ok((genotype, chr, pos)) => {
                    let gt = genotype_display(&genotype);

                    answer_parts.push(format!(
                        "\n**{}** in *{}*: Your genotype is **{}**\n{}",
                        rsid, gene, gt, description
                    ));

                    related_snps.push(RelatedSnp {
                        rsid: rsid.to_string(),
                        chromosome: chr,
                        position: pos,
                        genotype: gt,
                        gene: Some(gene.to_string()),
                        significance: Some(trait_name.to_string()),
                    });
                }
                Err(_) => {
                    answer_parts.push(format!(
                        "\n**{}** in *{}*: Not found in your data\n{}",
                        rsid, gene, description
                    ));
                }
            }
        }
    }

    sources.push(AnswerSource {
        source_type: "your_genome".to_string(),
        detail: "Your genomic data".to_string(),
    });
    sources.push(AnswerSource {
        source_type: "curated".to_string(),
        detail: "Curated trait-SNP associations".to_string(),
    });

    let confidence = if related_snps.is_empty() {
        "low"
    } else {
        "moderate"
    };

    Ok(GenomeAnswer {
        question: format!("What about my {}?", trait_name),
        answer: answer_parts.join("\n"),
        sources,
        related_snps,
        confidence: confidence.to_string(),
        disclaimer: MEDICAL_DISCLAIMER.to_string(),
    })
}

pub(crate) fn build_carrier_answer(
    conn: &Connection,
    genome_id: i64,
    condition: &str,
) -> Result<GenomeAnswer, AppError> {
    let mut sources = Vec::new();
    let mut related_snps = Vec::new();
    let mut answer_parts = Vec::new();

    let is_all = condition == "all";

    // Known carrier screening SNPs: (rsid, gene, condition, pathogenic_allele, description)
    let carrier_snps: Vec<(&str, &str, &str, &str, &str)> = vec![
        ("rs75961395", "CFTR", "cystic fibrosis", "A", "F508del — most common CF mutation"),
        ("rs113993960", "CFTR", "cystic fibrosis", "D", "Delta F508 deletion in CFTR"),
        ("rs334", "HBB", "sickle cell disease", "T", "HbS variant — sickle cell trait"),
        ("rs76723693", "HEXA", "tay-sachs disease", "C", "HEXA variant associated with Tay-Sachs"),
        ("rs1800562", "HFE", "hereditary hemochromatosis", "A", "C282Y — major hemochromatosis mutation"),
        ("rs1799945", "HFE", "hereditary hemochromatosis", "G", "H63D — secondary hemochromatosis variant"),
        ("rs80338939", "SMN1", "spinal muscular atrophy", "D", "SMN1 deletion"),
    ];

    let condition_lower = condition.to_lowercase();
    let screening_snps: Vec<&(&str, &str, &str, &str, &str)> = if is_all {
        carrier_snps.iter().collect()
    } else {
        carrier_snps
            .iter()
            .filter(|(_, _, cond, _, _)| cond.to_lowercase().contains(&condition_lower))
            .collect()
    };

    let display_condition = if is_all {
        "carrier status"
    } else {
        condition
    };
    answer_parts.push(format!(
        "**Carrier Screening — {}:**",
        capitalize(display_condition)
    ));

    let mut found_any = false;

    for &&(rsid, gene, cond_name, pathogenic_allele, description) in &screening_snps {
        let snp_result = conn.query_row(
            "SELECT genotype, chromosome, position FROM snps WHERE genome_id = ?1 AND rsid = ?2",
            rusqlite::params![genome_id, rsid],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        );

        match snp_result {
            Ok((genotype, chr, pos)) => {
                found_any = true;
                let gt = genotype_display(&genotype);

                let has_allele = genotype.contains(pathogenic_allele);
                let is_homozygous = genotype.len() == 2
                    && genotype[0..1] == *pathogenic_allele
                    && genotype[1..2] == *pathogenic_allele;

                let status = if is_homozygous {
                    "**Homozygous** — two copies of the variant allele detected"
                } else if has_allele {
                    "**Carrier** — one copy of the variant allele detected"
                } else {
                    "**Not a carrier** — variant allele not detected"
                };

                answer_parts.push(format!(
                    "\n**{}** (*{}*) — {}\n{} ({}) — {}\n{}",
                    cond_name, gene, rsid, gt, rsid, description, status
                ));

                related_snps.push(RelatedSnp {
                    rsid: rsid.to_string(),
                    chromosome: chr,
                    position: pos,
                    genotype: gt,
                    gene: Some(gene.to_string()),
                    significance: Some(format!("{} carrier screening", cond_name)),
                });
            }
            Err(_) => {
                answer_parts.push(format!(
                    "\n**{}** (*{}*) — {} not tested in your data",
                    cond_name, gene, rsid
                ));
            }
        }
    }

    if !found_any {
        answer_parts.push(format!(
            "\nNone of the screened carrier variants for {} were found in your genome data. This is common — most genotyping platforms test only a subset of carrier screening variants. Visit the Carrier Status page for a more complete analysis.",
            display_condition
        ));
    }

    sources.push(AnswerSource {
        source_type: "your_genome".to_string(),
        detail: "Your genomic data".to_string(),
    });
    sources.push(AnswerSource {
        source_type: "curated".to_string(),
        detail: "Known carrier screening variants".to_string(),
    });

    let confidence = if found_any { "moderate" } else { "low" };

    Ok(GenomeAnswer {
        question: format!("Carrier status for {}", display_condition),
        answer: answer_parts.join("\n"),
        sources,
        related_snps,
        confidence: confidence.to_string(),
        disclaimer: MEDICAL_DISCLAIMER.to_string(),
    })
}

pub(crate) fn build_chromosome_answer(
    conn: &Connection,
    genome_id: i64,
    chr: &str,
) -> Result<GenomeAnswer, AppError> {
    let mut sources = Vec::new();
    let mut related_snps = Vec::new();
    let mut answer_parts = Vec::new();

    let snp_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM snps WHERE genome_id = ?1 AND chromosome = ?2",
            rusqlite::params![genome_id, chr],
            |row| row.get(0),
        )
        .unwrap_or(0);

    answer_parts.push(format!(
        "**Chromosome {}:** {} variants in your data",
        chr,
        format_position(snp_count)
    ));

    let het_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM snps WHERE genome_id = ?1 AND chromosome = ?2
             AND LENGTH(genotype) = 2
             AND SUBSTR(genotype, 1, 1) != SUBSTR(genotype, 2, 1)",
            rusqlite::params![genome_id, chr],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let het_rate = if snp_count > 0 {
        (het_count as f64 / snp_count as f64 * 1000.0).round() / 10.0
    } else {
        0.0
    };

    answer_parts.push(format!("**Heterozygosity rate:** {:.1}%", het_rate));

    // Get notable annotated variants on this chromosome
    let notable = query_annotated_snps(
        conn,
        "SELECT s.rsid, s.chromosome, s.position, s.genotype,
                a.gene, a.clinical_significance, a.condition
         FROM snps s
         INNER JOIN annotations a ON s.rsid = a.rsid
         WHERE s.genome_id = ?1 AND s.chromosome = ?2
         AND a.clinical_significance IS NOT NULL
         ORDER BY
           CASE WHEN a.clinical_significance LIKE '%pathogenic%' THEN 0 ELSE 1 END,
           s.position
         LIMIT 15",
        &[
            &genome_id as &dyn rusqlite::types::ToSql,
            &chr as &dyn rusqlite::types::ToSql,
        ],
    )?;

    if !notable.is_empty() {
        answer_parts.push(format!("\n**Notable variants ({}):**", notable.len()));

        for (rsid, _chr_val, pos, genotype, gene, significance, condition) in &notable {
            let gt = genotype_display(genotype);

            let mut detail = format!("- **{}** at {} ({})", rsid, format_position(*pos), gt);
            if let Some(g) = gene {
                detail.push_str(&format!(" in *{}*", g));
            }
            if let Some(sig) = significance {
                detail.push_str(&format!(" — {}", sig));
            }
            if let Some(cond) = condition {
                detail.push_str(&format!(" ({})", cond));
            }
            answer_parts.push(detail);

            related_snps.push(RelatedSnp {
                rsid: rsid.clone(),
                chromosome: chr.to_string(),
                position: *pos,
                genotype: gt,
                gene: gene.clone(),
                significance: significance.clone(),
            });
        }

        sources.push(AnswerSource {
            source_type: "clinvar".to_string(),
            detail: format!("{} annotated variants", notable.len()),
        });
    } else {
        answer_parts.push(
            "\nNo clinically annotated variants on this chromosome. Download reference databases from Settings to get annotations."
                .to_string(),
        );
    }

    let gwas_count: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT g.rsid)
             FROM gwas_associations g
             INNER JOIN snps s ON g.rsid = s.rsid
             WHERE s.genome_id = ?1 AND s.chromosome = ?2",
            rusqlite::params![genome_id, chr],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if gwas_count > 0 {
        answer_parts.push(format!(
            "\n**GWAS:** {} variant{} with trait associations on this chromosome.",
            gwas_count,
            if gwas_count == 1 { "" } else { "s" }
        ));
        sources.push(AnswerSource {
            source_type: "gwas".to_string(),
            detail: format!("{} GWAS hits", gwas_count),
        });
    }

    sources.push(AnswerSource {
        source_type: "your_genome".to_string(),
        detail: format!("{} variants on chromosome {}", snp_count, chr),
    });

    let confidence = if !notable.is_empty() {
        "high"
    } else {
        "moderate"
    };

    Ok(GenomeAnswer {
        question: format!("What's on chromosome {}?", chr),
        answer: answer_parts.join("\n"),
        sources,
        related_snps,
        confidence: confidence.to_string(),
        disclaimer: MEDICAL_DISCLAIMER.to_string(),
    })
}

pub(crate) fn build_general_summary(
    conn: &Connection,
    genome_id: i64,
) -> Result<GenomeAnswer, AppError> {
    let mut sources = Vec::new();
    let mut related_snps = Vec::new();
    let mut answer_parts = Vec::new();

    let total_snps: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM snps WHERE genome_id = ?1",
            [genome_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

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

    let het_rate = if total_diploid > 0 {
        (het_count as f64 / total_diploid as f64 * 1000.0).round() / 10.0
    } else {
        0.0
    };

    answer_parts.push("**Genome Summary**".to_string());
    answer_parts.push(format!(
        "Your genome contains **{}** variants with a heterozygosity rate of **{:.1}%**.",
        format_position(total_snps),
        het_rate
    ));

    let annotated_count: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT s.rsid) FROM snps s
             INNER JOIN annotations a ON s.rsid = a.rsid
             WHERE s.genome_id = ?1",
            [genome_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if annotated_count > 0 {
        answer_parts.push(format!(
            "**Annotated variants:** {} of your variants have clinical annotations.",
            format_position(annotated_count)
        ));
    }

    let pathogenic_count: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT s.rsid) FROM snps s
             INNER JOIN annotations a ON s.rsid = a.rsid
             WHERE s.genome_id = ?1
             AND LOWER(a.clinical_significance) LIKE '%pathogenic%'",
            [genome_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if pathogenic_count > 0 {
        answer_parts.push(format!(
            "**Pathogenic variants:** {} variant{} flagged as pathogenic or likely pathogenic.",
            pathogenic_count,
            if pathogenic_count == 1 { "" } else { "s" }
        ));

        // Show top pathogenic variants
        let pathogenic_snps = query_annotated_snps(
            conn,
            "SELECT s.rsid, s.chromosome, s.position, s.genotype,
                    a.gene, a.clinical_significance, a.condition
             FROM snps s
             INNER JOIN annotations a ON s.rsid = a.rsid
             WHERE s.genome_id = ?1
             AND LOWER(a.clinical_significance) LIKE '%pathogenic%'
             ORDER BY s.chromosome, s.position
             LIMIT 5",
            &[&genome_id as &dyn rusqlite::types::ToSql],
        )?;

        for (rsid, chr, pos, genotype, gene, significance, condition) in &pathogenic_snps {
            let gt = genotype_display(genotype);

            let mut detail = format!("- **{}** ({})", rsid, gt);
            if let Some(g) = gene {
                detail.push_str(&format!(" in *{}*", g));
            }
            if let Some(cond) = condition {
                detail.push_str(&format!(" — {}", cond));
            }
            answer_parts.push(detail);

            related_snps.push(RelatedSnp {
                rsid: rsid.clone(),
                chromosome: chr.clone(),
                position: *pos,
                genotype: gt,
                gene: gene.clone(),
                significance: significance.clone(),
            });
        }
    }

    let gwas_count: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT g.rsid) FROM gwas_associations g
             INNER JOIN snps s ON g.rsid = s.rsid
             WHERE s.genome_id = ?1",
            [genome_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if gwas_count > 0 {
        answer_parts.push(format!(
            "\n**GWAS associations:** {} of your variants have been linked to traits or conditions in genome-wide association studies.",
            format_position(gwas_count)
        ));
    }

    let snpedia_count: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT se.rsid) FROM snpedia_entries se
             INNER JOIN snps s ON se.rsid = s.rsid
             WHERE s.genome_id = ?1",
            [genome_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if snpedia_count > 0 {
        answer_parts.push(format!(
            "**SNPedia:** {} of your variants have community-curated information.",
            format_position(snpedia_count)
        ));
    }

    answer_parts.push("\n**Explore further:**".to_string());
    answer_parts.push("- Ask about specific genes (e.g., \"What about APOE?\")".to_string());
    answer_parts.push("- Check health risks (e.g., \"Am I at risk for diabetes?\")".to_string());
    answer_parts.push("- Look up traits (e.g., \"Eye color genetics\")".to_string());
    answer_parts.push(
        "- Check drug metabolism (e.g., \"How do I metabolize caffeine?\")".to_string(),
    );
    answer_parts
        .push("- Review carrier status (e.g., \"Am I a carrier for anything?\")".to_string());

    sources.push(AnswerSource {
        source_type: "your_genome".to_string(),
        detail: format!("{} total variants", total_snps),
    });
    if annotated_count > 0 {
        sources.push(AnswerSource {
            source_type: "clinvar".to_string(),
            detail: format!("{} annotations", annotated_count),
        });
    }
    if gwas_count > 0 {
        sources.push(AnswerSource {
            source_type: "gwas".to_string(),
            detail: format!("{} GWAS associations", gwas_count),
        });
    }
    if snpedia_count > 0 {
        sources.push(AnswerSource {
            source_type: "snpedia".to_string(),
            detail: format!("{} SNPedia entries", snpedia_count),
        });
    }

    Ok(GenomeAnswer {
        question: "Summarize my genome".to_string(),
        answer: answer_parts.join("\n"),
        sources,
        related_snps,
        confidence: "high".to_string(),
        disclaimer: MEDICAL_DISCLAIMER.to_string(),
    })
}

pub(crate) fn build_unknown_answer(question: &str) -> GenomeAnswer {
    let answer = format!(
        "**I'm not sure how to answer that question.**\n\nI can help you explore your genome data. Try asking about:\n\n- **Specific variants:** \"What is rs12913832?\" or \"Tell me about rs1801133\"\n- **Genes:** \"What about BRCA1?\" or \"Do I have MTHFR variants?\"\n- **Health risks:** \"Am I at risk for diabetes?\" or \"What about celiac disease?\"\n- **Drug responses:** \"How do I metabolize caffeine?\" or \"Is clopidogrel safe for me?\"\n- **Traits:** \"What's my eye color?\" or \"Am I lactose intolerant?\"\n- **Carrier status:** \"Am I a carrier for cystic fibrosis?\"\n- **Chromosomes:** \"What's on chromosome 6?\"\n- **Overview:** \"Summarize my genome\"\n\nYour question: *\"{}\"*",
        question
    );

    GenomeAnswer {
        question: question.to_string(),
        answer,
        sources: vec![],
        related_snps: vec![],
        confidence: "low".to_string(),
        disclaimer: MEDICAL_DISCLAIMER.to_string(),
    }
}

// ── Tauri Command ───────────────────────────────────────────────

#[tauri::command]
pub fn ask_genome(
    question: String,
    genome_id: i64,
    db: State<'_, Database>,
) -> Result<GenomeAnswer, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;

    let intent = parse_question(&question);

    log::info!("Ask genome: {:?} -> {:?}", question, intent);

    let mut answer = match intent {
        QueryIntent::RsidLookup(rsid) => build_rsid_answer(&conn, genome_id, &rsid)?,
        QueryIntent::GeneLookup(gene) => build_gene_answer(&conn, genome_id, &gene)?,
        QueryIntent::ConditionRisk(condition) => {
            build_condition_risk_answer(&conn, genome_id, &condition)?
        }
        QueryIntent::DrugResponse(drug) => {
            build_drug_response_answer(&conn, genome_id, &drug)?
        }
        QueryIntent::TraitQuery(trait_name) => {
            build_trait_answer(&conn, genome_id, &trait_name)?
        }
        QueryIntent::CarrierQuery(condition) => {
            build_carrier_answer(&conn, genome_id, &condition)?
        }
        QueryIntent::ChromosomeQuery(chr) => {
            build_chromosome_answer(&conn, genome_id, &chr)?
        }
        QueryIntent::GeneralSummary => build_general_summary(&conn, genome_id)?,
        QueryIntent::Unknown(q) => build_unknown_answer(&q),
    };

    // Always override the question with the original
    answer.question = question;

    Ok(answer)
}
