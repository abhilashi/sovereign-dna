use csv::ReaderBuilder;

use crate::error::AppError;
use super::{ParseResult, ParsedSnp};

/// Parse an AncestryDNA raw data file.
///
/// Format: tab-delimited with header comments starting with `#`.
/// Columns: rsid, chromosome, position, allele1, allele2.
/// The two alleles are combined into a single genotype string.
pub fn parse_ancestry(content: &str) -> Result<ParseResult, AppError> {
    let mut snps = Vec::with_capacity(700_000);
    let mut total_lines: usize = 0;
    let mut skipped_lines: usize = 0;
    let mut build: Option<String> = None;

    // Pre-scan comments for build info
    for line in content.lines() {
        if line.starts_with('#') {
            if line.contains("build 37") || line.contains("GRCh37") || line.contains("hg19") {
                build = Some("GRCh37".to_string());
            } else if line.contains("build 38") || line.contains("GRCh38") || line.contains("hg38") {
                build = Some("GRCh38".to_string());
            }
        }
    }

    // Filter out comment lines
    let data_lines: String = content
        .lines()
        .filter(|line| {
            total_lines += 1;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                skipped_lines += 1;
                return false;
            }
            true
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut rdr = ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(false)
        .flexible(true)
        .from_reader(data_lines.as_bytes());

    for result in rdr.records() {
        let record = match result {
            Ok(r) => r,
            Err(_) => {
                skipped_lines += 1;
                continue;
            }
        };

        if record.len() < 5 {
            skipped_lines += 1;
            continue;
        }

        let rsid = record[0].trim();
        let chromosome = record[1].trim();
        let position_str = record[2].trim();
        let allele1 = record[3].trim();
        let allele2 = record[4].trim();

        // Skip header rows
        if rsid == "rsid" || rsid == "RSID" || rsid.starts_with("rsid") {
            skipped_lines += 1;
            continue;
        }

        let position: i64 = match position_str.parse() {
            Ok(p) => p,
            Err(_) => {
                skipped_lines += 1;
                continue;
            }
        };

        // Skip no-calls
        if allele1 == "0" && allele2 == "0" {
            skipped_lines += 1;
            continue;
        }

        let chrom = chromosome
            .strip_prefix("chr")
            .unwrap_or(chromosome)
            .to_uppercase();

        let genotype = format!("{}{}", allele1.to_uppercase(), allele2.to_uppercase());

        snps.push(ParsedSnp {
            rsid: rsid.to_string(),
            chromosome: chrom,
            position,
            genotype,
        });
    }

    Ok(ParseResult {
        format: "ancestry".to_string(),
        build,
        total_lines,
        skipped_lines,
        snps,
    })
}
