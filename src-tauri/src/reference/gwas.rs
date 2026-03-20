use std::collections::HashSet;

use crate::db::queries::GwasAssociation;
use crate::error::AppError;

/// Column indices for GWAS Catalog TSV.
struct GwasHeaderIndices {
    snps: usize,
    disease_trait: usize,
    p_value: usize,
    or_beta: usize,
    risk_allele: usize,
    study_accession: usize,
    pubmed_id: usize,
    sample_size: usize,
}

impl Default for GwasHeaderIndices {
    fn default() -> Self {
        Self {
            snps: 21,
            disease_trait: 7,
            p_value: 27,
            or_beta: 30,
            risk_allele: 20,
            study_accession: 36,
            pubmed_id: 1,
            sample_size: 8,
        }
    }
}

/// Parse the GWAS Catalog TSV content, filtering to only rsIDs present in the user's genome.
///
/// The GWAS Catalog is a tab-separated file downloaded from:
/// `https://www.ebi.ac.uk/gwas/api/search/downloads/full`
///
/// Important columns are detected by header name, not fixed index.
pub fn parse_gwas_catalog(
    content: &str,
    user_rsids: &HashSet<String>,
) -> Result<Vec<GwasAssociation>, AppError> {
    let mut associations = Vec::new();
    let mut header_indices: Option<GwasHeaderIndices> = None;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        // Detect header line
        if header_indices.is_none()
            && (trimmed.contains("DISEASE/TRAIT") || trimmed.contains("SNPS"))
        {
            header_indices = Some(parse_gwas_header(trimmed));
            continue;
        }

        let indices = match &header_indices {
            Some(i) => i,
            None => continue,
        };

        let fields: Vec<&str> = trimmed.split('\t').collect();

        // Get rsID(s) from the SNPS column
        let snps_raw = get_field(&fields, indices.snps);
        if snps_raw.is_empty() {
            continue;
        }

        // SNPS column may contain multiple rsIDs separated by various delimiters
        let rsids: Vec<String> = snps_raw
            .split(|c: char| c == ';' || c == ',' || c == ' ' || c == 'x')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| s.starts_with("rs"))
            .collect();

        if rsids.is_empty() {
            continue;
        }

        // Get trait name
        let trait_name = get_field(&fields, indices.disease_trait);
        if trait_name.is_empty() {
            continue;
        }

        // Parse p-value
        let p_value = parse_float(get_field(&fields, indices.p_value));

        // Parse odds ratio / beta
        let odds_ratio = parse_float(get_field(&fields, indices.or_beta));

        // Risk allele
        let risk_allele_raw = get_field(&fields, indices.risk_allele);
        let risk_allele = if risk_allele_raw.is_empty() || risk_allele_raw == "NR" {
            None
        } else {
            Some(risk_allele_raw.to_string())
        };

        // Study accession
        let study_accession_raw = get_field(&fields, indices.study_accession);
        let study_accession = if study_accession_raw.is_empty() {
            None
        } else {
            Some(study_accession_raw.to_string())
        };

        // PubMed ID
        let pubmed_id_raw = get_field(&fields, indices.pubmed_id);
        let pubmed_id = if pubmed_id_raw.is_empty() {
            None
        } else {
            Some(pubmed_id_raw.to_string())
        };

        // Sample size - try to parse first number from the text
        let sample_size = parse_sample_size(get_field(&fields, indices.sample_size));

        // Create an association for each rsID that matches the user's genome
        for rsid in &rsids {
            if !user_rsids.contains(rsid) {
                continue;
            }

            associations.push(GwasAssociation {
                id: None,
                rsid: rsid.clone(),
                trait_name: trait_name.to_string(),
                p_value,
                odds_ratio,
                risk_allele: risk_allele.clone(),
                study_accession: study_accession.clone(),
                pubmed_id: pubmed_id.clone(),
                sample_size,
                source: "GWAS Catalog".to_string(),
            });
        }
    }

    log::info!(
        "Parsed {} GWAS associations matching user genome",
        associations.len()
    );

    Ok(associations)
}

fn parse_gwas_header(header_line: &str) -> GwasHeaderIndices {
    let columns: Vec<&str> = header_line.split('\t').collect();
    let mut indices = GwasHeaderIndices::default();

    for (i, col) in columns.iter().enumerate() {
        let col_upper = col.trim().to_uppercase();
        match col_upper.as_str() {
            "SNPS" => indices.snps = i,
            "DISEASE/TRAIT" => indices.disease_trait = i,
            "P-VALUE" => indices.p_value = i,
            "OR OR BETA" | "OR or BETA" => indices.or_beta = i,
            "STRONGEST SNP-RISK ALLELE" => indices.risk_allele = i,
            "STUDY ACCESSION" => indices.study_accession = i,
            "PUBMEDID" => indices.pubmed_id = i,
            "INITIAL SAMPLE SIZE" => indices.sample_size = i,
            _ => {}
        }
    }

    indices
}

fn get_field<'a>(fields: &[&'a str], index: usize) -> &'a str {
    fields.get(index).copied().unwrap_or("")
}

fn parse_float(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed == "NR" || trimmed == "NA" || trimmed == "-" {
        return None;
    }
    trimmed.parse::<f64>().ok()
}

/// Try to extract an integer sample size from the "INITIAL SAMPLE SIZE" text field.
/// The field typically contains text like "1,234 European ancestry cases, 5,678 controls".
/// We extract the first number we find.
fn parse_sample_size(s: &str) -> Option<i64> {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed == "NR" || trimmed == "NA" {
        return None;
    }

    // Remove commas and try to find a number
    let cleaned: String = trimmed.chars().filter(|c| c.is_ascii_digit() || *c == ' ').collect();
    let parts: Vec<&str> = cleaned.split_whitespace().collect();

    // Sum all numbers found (total sample size)
    let mut total: i64 = 0;
    let mut found_any = false;
    for part in parts {
        if let Ok(n) = part.parse::<i64>() {
            total += n;
            found_any = true;
        }
    }

    if found_any {
        Some(total)
    } else {
        None
    }
}
