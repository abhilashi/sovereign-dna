use crate::error::AppError;
use super::{ParseResult, ParsedSnp};

/// Parse a VCF (Variant Call Format) file.
///
/// Meta-information lines start with `##`.
/// The header line starts with `#CHROM`.
/// Data lines contain: CHROM, POS, ID, REF, ALT, QUAL, FILTER, INFO, FORMAT, SAMPLE...
/// Genotype is extracted from the first sample column using the GT field.
pub fn parse_vcf(content: &str) -> Result<ParseResult, AppError> {
    let mut snps = Vec::with_capacity(700_000);
    let mut total_lines: usize = 0;
    let mut skipped_lines: usize = 0;
    let mut build: Option<String> = None;
    let mut _header_found = false;

    for line in content.lines() {
        total_lines += 1;
        let trimmed = line.trim();

        if trimmed.is_empty() {
            skipped_lines += 1;
            continue;
        }

        // Meta-information lines
        if trimmed.starts_with("##") {
            skipped_lines += 1;

            // Extract reference build from meta lines
            if trimmed.contains("GRCh37") || trimmed.contains("hg19") {
                build = Some("GRCh37".to_string());
            } else if trimmed.contains("GRCh38") || trimmed.contains("hg38") {
                build = Some("GRCh38".to_string());
            }
            continue;
        }

        // Header line
        if trimmed.starts_with("#CHROM") {
            _header_found = true;
            skipped_lines += 1;
            continue;
        }

        // Skip any remaining comment lines
        if trimmed.starts_with('#') {
            skipped_lines += 1;
            continue;
        }

        // Data line
        let fields: Vec<&str> = trimmed.split('\t').collect();
        if fields.len() < 10 {
            skipped_lines += 1;
            continue;
        }

        let chrom = fields[0]
            .strip_prefix("chr")
            .unwrap_or(fields[0])
            .to_uppercase();
        let pos_str = fields[1];
        let rsid = fields[2];
        let ref_allele = fields[3];
        let alt_allele = fields[4];
        let format_field = fields[8];
        let sample_field = fields[9];

        // Parse position
        let position: i64 = match pos_str.parse() {
            Ok(p) => p,
            Err(_) => {
                skipped_lines += 1;
                continue;
            }
        };

        // Skip entries without rs IDs (use "." or other ID)
        let rsid_str = if rsid == "." {
            format!("chr{}:{}", chrom, position)
        } else {
            rsid.to_string()
        };

        // Parse genotype from FORMAT and SAMPLE fields
        // Find the GT field index in FORMAT
        let format_parts: Vec<&str> = format_field.split(':').collect();
        let gt_index = format_parts.iter().position(|&f| f == "GT");

        let genotype = if let Some(idx) = gt_index {
            let sample_parts: Vec<&str> = sample_field.split(':').collect();
            if idx < sample_parts.len() {
                let gt = sample_parts[idx];
                // Convert VCF GT notation (0/1, 1/1, etc.) to allele strings
                vcf_gt_to_genotype(gt, ref_allele, alt_allele)
            } else {
                skipped_lines += 1;
                continue;
            }
        } else {
            skipped_lines += 1;
            continue;
        };

        if genotype.is_empty() || genotype == ".." {
            skipped_lines += 1;
            continue;
        }

        snps.push(ParsedSnp {
            rsid: rsid_str,
            chromosome: chrom,
            position,
            genotype,
        });
    }

    Ok(ParseResult {
        format: "vcf".to_string(),
        build,
        total_lines,
        skipped_lines,
        snps,
    })
}

/// Convert VCF genotype notation (e.g., "0/1", "1|1") to allele string.
fn vcf_gt_to_genotype(gt: &str, ref_allele: &str, alt_allele: &str) -> String {
    let separator = if gt.contains('|') { '|' } else { '/' };
    let alleles: Vec<&str> = gt.split(separator).collect();

    if alleles.len() != 2 {
        return String::new();
    }

    let alt_alleles: Vec<&str> = alt_allele.split(',').collect();

    let mut result = String::with_capacity(2);
    for allele_str in &alleles {
        match *allele_str {
            "." => return String::new(),
            "0" => {
                // For simple SNPs, take just the first character
                if let Some(c) = ref_allele.chars().next() {
                    result.push(c);
                }
            }
            _ => {
                if let Ok(idx) = allele_str.parse::<usize>() {
                    if idx > 0 && idx <= alt_alleles.len() {
                        if let Some(c) = alt_alleles[idx - 1].chars().next() {
                            result.push(c);
                        }
                    }
                }
            }
        }
    }

    result.to_uppercase()
}
