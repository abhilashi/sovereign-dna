use std::io::{BufRead, Cursor};

use crate::error::AppError;

use super::streaming::detect_build_line;
use super::{GenomeParser, ParseResult, ParseSummary, ParsedSnp, SnpSink, VecSink};

/// Streaming parser for AncestryDNA raw data files.
///
/// Format: tab-delimited, header/comment lines start with `#`.
/// Columns: `rsid`, `chromosome`, `position`, `allele1`, `allele2`.
/// The two alleles are concatenated into a single genotype string.
pub struct AncestryParser;

impl GenomeParser for AncestryParser {
    fn parse_streaming(
        &self,
        reader: &mut dyn BufRead,
        sink: &mut dyn SnpSink,
    ) -> Result<ParseSummary, AppError> {
        let mut total_lines: usize = 0;
        let mut skipped_lines: usize = 0;
        let mut snp_count: usize = 0;
        let mut build: Option<String> = None;

        let mut line = String::new();
        loop {
            line.clear();
            let bytes = reader
                .read_line(&mut line)
                .map_err(|e| AppError::Io(e.to_string()))?;
            if bytes == 0 {
                break; // EOF
            }
            total_lines += 1;
            let trimmed = line.trim();

            if trimmed.is_empty() {
                skipped_lines += 1;
                continue;
            }

            if trimmed.starts_with('#') {
                skipped_lines += 1;
                if build.is_none() {
                    build = detect_build_line(trimmed);
                }
                continue;
            }

            let mut cols = trimmed.split('\t');
            let rsid = cols.next().unwrap_or("").trim();
            let chromosome = cols.next().unwrap_or("").trim();
            let position_str = cols.next().unwrap_or("").trim();
            let allele1 = cols.next().unwrap_or("").trim();
            let allele2 = cols.next().unwrap_or("").trim();

            if rsid.is_empty()
                || chromosome.is_empty()
                || position_str.is_empty()
                || allele1.is_empty()
                || allele2.is_empty()
            {
                skipped_lines += 1;
                continue;
            }

            // Skip the column-header row (`rsid  chromosome  position ...`).
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

            // Skip no-calls (AncestryDNA encodes these as "0"/"0").
            if allele1 == "0" && allele2 == "0" {
                skipped_lines += 1;
                continue;
            }

            let chrom = chromosome
                .strip_prefix("chr")
                .unwrap_or(chromosome)
                .to_uppercase();
            let genotype = format!("{}{}", allele1.to_uppercase(), allele2.to_uppercase());

            sink.push(ParsedSnp {
                rsid: rsid.to_string(),
                chromosome: chrom,
                position,
                genotype,
                // Genotyping-array formats have no REF/ALT or sample dimension.
                ref_allele: None,
                alt_allele: None,
                sample: None,
            })?;
            snp_count += 1;
        }

        Ok(ParseSummary {
            format: "ancestry".to_string(),
            build,
            total_lines,
            skipped_lines,
            snp_count,
        })
    }
}

/// Parse an AncestryDNA raw data file from an in-memory string.
///
/// Backward-compatible convenience wrapper around the streaming
/// [`AncestryParser`]. Prefer
/// [`crate::parser::streaming::parse_path_streaming`] for large inputs.
pub fn parse_ancestry(content: &str) -> Result<ParseResult, AppError> {
    let mut sink = VecSink::default();
    let mut reader = Cursor::new(content.as_bytes());
    let summary = AncestryParser.parse_streaming(&mut reader, &mut sink)?;
    Ok(ParseResult {
        format: summary.format,
        build: summary.build,
        total_lines: summary.total_lines,
        skipped_lines: summary.skipped_lines,
        snps: sink.snps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "#AncestryDNA raw data download\n\
# build 37\n\
rsid\tchromosome\tposition\tallele1\tallele2\n\
rs4477212\t1\t82154\tA\tA\n\
rs3094315\t1\t752566\tA\tG\n\
rs3131972\t1\t752721\t0\t0\n\
rs12124819\t1\t776546\tG\tG\n";

    #[test]
    fn streaming_parses_synthetic_ancestry() {
        let mut sink = VecSink::default();
        let mut reader = Cursor::new(SAMPLE.as_bytes());
        let summary = AncestryParser
            .parse_streaming(&mut reader, &mut sink)
            .unwrap();

        // 4 data rows, one no-call ("0"/"0") skipped → 3 kept.
        assert_eq!(summary.snp_count, 3);
        assert_eq!(sink.snps.len(), 3);
        assert_eq!(summary.build.as_deref(), Some("GRCh37"));
        assert_eq!(sink.snps[0].genotype, "AA");
        assert_eq!(sink.snps[1].genotype, "AG");
    }

    #[test]
    fn wrapper_matches_streaming() {
        let result = parse_ancestry(SAMPLE).unwrap();
        assert_eq!(result.format, "ancestry");
        assert_eq!(result.snps.len(), 3);
    }
}
