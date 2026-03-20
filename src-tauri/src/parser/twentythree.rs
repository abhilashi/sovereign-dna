use csv::ReaderBuilder;

use crate::error::AppError;
use super::{ParseResult, ParsedSnp};

/// Parse a 23andMe raw data file (both v3 and v5 formats).
///
/// Format: tab-delimited with header comments starting with `#`.
/// Columns: rsid, chromosome, position, genotype.
/// v5 files have an explicit header line after comments; v3 may not.
pub fn parse_23andme(content: &str, detected_version: &str) -> Result<ParseResult, AppError> {
    let mut snps = Vec::with_capacity(700_000);
    let mut total_lines: usize = 0;
    let mut skipped_lines: usize = 0;
    let mut build: Option<String> = None;

    // Pre-scan comment lines for build information
    for line in content.lines() {
        if line.starts_with('#') {
            if line.contains("build 37") || line.contains("GRCh37") || line.contains("hg19") {
                build = Some("GRCh37".to_string());
            } else if line.contains("build 38") || line.contains("GRCh38") || line.contains("hg38") {
                build = Some("GRCh38".to_string());
            } else if line.contains("build 36") || line.contains("GRCh36") || line.contains("hg18") {
                build = Some("GRCh36".to_string());
            }
        }
    }

    // Filter out comment lines, collect data lines
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

    // Use the csv crate for efficient tab-delimited parsing
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

        if record.len() < 4 {
            skipped_lines += 1;
            continue;
        }

        let rsid = record[0].trim();
        let chromosome = record[1].trim();
        let position_str = record[2].trim();
        let genotype = record[3].trim();

        // Skip header-like rows
        if rsid == "rsid" || rsid == "# rsid" {
            skipped_lines += 1;
            continue;
        }

        // Parse position as integer
        let position: i64 = match position_str.parse() {
            Ok(p) => p,
            Err(_) => {
                skipped_lines += 1;
                continue;
            }
        };

        // Skip entries with no genotype or marked as deleted/no-call
        if genotype.is_empty() || genotype == "--" || genotype == "00" {
            skipped_lines += 1;
            continue;
        }

        // Normalize chromosome (remove "chr" prefix if present)
        let chrom = chromosome
            .strip_prefix("chr")
            .unwrap_or(chromosome)
            .to_uppercase();

        snps.push(ParsedSnp {
            rsid: rsid.to_string(),
            chromosome: chrom,
            position,
            genotype: genotype.to_uppercase(),
        });
    }

    Ok(ParseResult {
        format: detected_version.to_string(),
        build,
        total_lines,
        skipped_lines,
        snps,
    })
}
