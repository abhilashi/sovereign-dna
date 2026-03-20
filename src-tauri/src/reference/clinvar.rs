use std::collections::HashSet;
use std::path::Path;

use crate::db::queries::Annotation;
use crate::error::AppError;

/// Accepted clinical significance values.
const ACCEPTED_SIGNIFICANCES: &[&str] = &[
    "pathogenic",
    "likely pathogenic",
    "benign",
    "likely benign",
    "uncertain significance",
    "pathogenic/likely pathogenic",
    "benign/likely benign",
];

/// Parse ClinVar variant_summary.txt content and extract annotations.
///
/// The content is tab-delimited with columns including:
/// #AlleleID, Type, Name, GeneID, GeneSymbol, HGNC_ID, ClinicalSignificance,
/// ClinSigSimple, LastEvaluated, RS# (dbSNP), nsv/esv (dbVar), ...
///
/// We extract: RS# (as rsid), GeneSymbol, ClinicalSignificance, PhenotypeList (condition),
/// ReviewStatus, and other relevant fields.
///
/// When `user_rsids` is provided, only annotations whose rsID is in the set are kept.
/// This dramatically reduces the data stored (from ~700K entries to ~50-100K matching the user's SNPs).
pub fn parse_clinvar(content: &str, user_rsids: &HashSet<String>) -> Result<Vec<Annotation>, AppError> {
    let mut annotations = Vec::new();
    let mut header_indices: Option<HeaderIndices> = None;
    let filter_by_user = !user_rsids.is_empty();

    for line in content.lines() {
        let trimmed = line.trim();

        // Parse header to find column indices
        if trimmed.starts_with('#') || trimmed.starts_with("AlleleID") {
            let clean = trimmed.trim_start_matches('#');
            header_indices = Some(parse_header(clean));
            continue;
        }

        if trimmed.is_empty() {
            continue;
        }

        let indices = match &header_indices {
            Some(i) => i,
            None => {
                // If no header found yet, try with default column positions
                header_indices = Some(HeaderIndices::default());
                header_indices.as_ref().unwrap()
            }
        };

        let fields: Vec<&str> = trimmed.split('\t').collect();

        // Get RS number
        let rsid_raw = get_field(&fields, indices.rs_dbsnp);
        if rsid_raw.is_empty() || rsid_raw == "-1" || rsid_raw == "-" {
            continue;
        }

        let rsid = if rsid_raw.starts_with("rs") {
            rsid_raw.to_string()
        } else {
            format!("rs{}", rsid_raw)
        };

        // Filter to only rsIDs in user's genome (if filtering is enabled)
        if filter_by_user && !user_rsids.contains(&rsid) {
            continue;
        }

        // Get clinical significance
        let significance = get_field(&fields, indices.clinical_significance).to_lowercase();

        // Filter to only accepted significance values
        if !ACCEPTED_SIGNIFICANCES.iter().any(|s| significance.contains(s)) {
            continue;
        }

        let gene = get_field(&fields, indices.gene_symbol);
        let condition = get_field(&fields, indices.phenotype_list);
        let review_status = get_field(&fields, indices.review_status);

        annotations.push(Annotation {
            rsid,
            gene: if gene.is_empty() || gene == "-" {
                None
            } else {
                Some(gene.to_string())
            },
            clinical_significance: Some(significance),
            condition: if condition.is_empty() || condition == "-" {
                None
            } else {
                Some(condition.to_string())
            },
            review_status: if review_status.is_empty() || review_status == "-" {
                None
            } else {
                Some(review_status.to_string())
            },
            allele_frequency: None,
            source: Some("ClinVar".to_string()),
        });
    }

    // Deduplicate by rsid, keeping the first (usually most severe) entry
    let mut seen = std::collections::HashSet::new();
    annotations.retain(|a| seen.insert(a.rsid.clone()));

    log::info!("Parsed {} annotations from ClinVar", annotations.len());

    Ok(annotations)
}

/// Parse ClinVar from a file path (convenience wrapper for backward compatibility).
pub fn parse_clinvar_file(path: &Path) -> Result<Vec<Annotation>, AppError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| AppError::Io(format!("Failed to read ClinVar file {}: {}", path.display(), e)))?;

    let empty_set = HashSet::new();
    parse_clinvar(&content, &empty_set)
}

struct HeaderIndices {
    rs_dbsnp: usize,
    gene_symbol: usize,
    clinical_significance: usize,
    phenotype_list: usize,
    review_status: usize,
}

impl Default for HeaderIndices {
    fn default() -> Self {
        // Default column positions for standard ClinVar variant_summary.txt
        Self {
            rs_dbsnp: 9,
            gene_symbol: 4,
            clinical_significance: 6,
            phenotype_list: 13,
            review_status: 24,
        }
    }
}

fn parse_header(header_line: &str) -> HeaderIndices {
    let columns: Vec<&str> = header_line.split('\t').collect();
    let mut indices = HeaderIndices::default();

    for (i, col) in columns.iter().enumerate() {
        let col_lower = col.trim().to_lowercase();
        match col_lower.as_str() {
            "rs# (dbsnp)" | "rs#(dbsnp)" | "rsid" => indices.rs_dbsnp = i,
            "genesymbol" | "gene_symbol" | "gene" => indices.gene_symbol = i,
            "clinicalsignificance" | "clinical_significance" | "clinsig" => {
                indices.clinical_significance = i
            }
            "phenotypelist" | "phenotype_list" | "condition" => indices.phenotype_list = i,
            "reviewstatus" | "review_status" => indices.review_status = i,
            _ => {}
        }
    }

    indices
}

fn get_field<'a>(fields: &[&'a str], index: usize) -> &'a str {
    fields.get(index).copied().unwrap_or("")
}
