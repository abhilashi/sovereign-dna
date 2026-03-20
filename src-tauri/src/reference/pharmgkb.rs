use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// A pharmacogenomic annotation from PharmGKB.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PharmaAnnotation {
    pub rsid: String,
    pub gene: String,
    pub drug: String,
    pub phenotype_category: String,
    pub evidence_level: String,
}

/// Parse PharmGKB clinical annotations file.
///
/// The file is tab-delimited with columns including:
/// Clinical Annotation ID, Variant/Haplotypes, Gene, Level of Evidence,
/// Phenotype Category, Drug(s), Phenotype(s), ...
pub fn parse_pharmgkb(path: &Path) -> Result<Vec<PharmaAnnotation>, AppError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        AppError::Io(format!(
            "Failed to read PharmGKB file {}: {}",
            path.display(),
            e
        ))
    })?;

    let mut annotations = Vec::new();
    let mut header_found = false;
    let mut col_variant = 1usize;
    let mut col_gene = 2usize;
    let mut col_evidence = 3usize;
    let mut col_phenotype_cat = 4usize;
    let mut col_drug = 5usize;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        // Detect header row
        if !header_found
            && (trimmed.contains("Variant") || trimmed.contains("Clinical Annotation"))
        {
            let cols: Vec<&str> = trimmed.split('\t').collect();
            for (i, col) in cols.iter().enumerate() {
                let lower = col.to_lowercase();
                if lower.contains("variant") || lower.contains("haplotype") {
                    col_variant = i;
                } else if lower == "gene" || lower.contains("gene") && !lower.contains("pheno") {
                    col_gene = i;
                } else if lower.contains("level") || lower.contains("evidence") {
                    col_evidence = i;
                } else if lower.contains("phenotype") && lower.contains("categor") {
                    col_phenotype_cat = i;
                } else if lower.contains("drug") {
                    col_drug = i;
                }
            }
            header_found = true;
            continue;
        }

        if !header_found {
            continue;
        }

        let fields: Vec<&str> = trimmed.split('\t').collect();

        let variant = fields.get(col_variant).copied().unwrap_or("").trim();
        let gene = fields.get(col_gene).copied().unwrap_or("").trim();
        let evidence = fields.get(col_evidence).copied().unwrap_or("").trim();
        let phenotype_cat = fields.get(col_phenotype_cat).copied().unwrap_or("").trim();
        let drug = fields.get(col_drug).copied().unwrap_or("").trim();

        // Extract rsIDs from the variant field (may contain multiple)
        let rsids: Vec<&str> = variant
            .split(|c: char| c == ',' || c == ';' || c == '/')
            .map(|s| s.trim())
            .filter(|s| s.starts_with("rs"))
            .collect();

        if rsids.is_empty() {
            continue;
        }

        // Drugs field may contain multiple drugs separated by semicolons or commas
        let drugs: Vec<&str> = drug
            .split(|c: char| c == ';' || c == ',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        for rsid in &rsids {
            for drug_name in &drugs {
                annotations.push(PharmaAnnotation {
                    rsid: rsid.to_string(),
                    gene: gene.to_string(),
                    drug: drug_name.to_string(),
                    phenotype_category: phenotype_cat.to_string(),
                    evidence_level: evidence.to_string(),
                });
            }

            // If no drugs listed, still create entry for the gene
            if drugs.is_empty() && !gene.is_empty() {
                annotations.push(PharmaAnnotation {
                    rsid: rsid.to_string(),
                    gene: gene.to_string(),
                    drug: String::new(),
                    phenotype_category: phenotype_cat.to_string(),
                    evidence_level: evidence.to_string(),
                });
            }
        }
    }

    log::info!(
        "Parsed {} pharmacogenomic annotations from PharmGKB",
        annotations.len()
    );

    Ok(annotations)
}
