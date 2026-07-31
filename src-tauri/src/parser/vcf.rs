use std::io::{BufRead, Cursor};

use crate::error::AppError;

use super::streaming::detect_build_line;
use super::{GenomeParser, ParseResult, ParseSummary, ParsedSnp, SnpSink, VecSink};

/// Streaming parser for VCF (Variant Call Format) files.
///
/// Meta-information lines start with `##`; the header line starts with `#CHROM`.
/// Data columns: CHROM, POS, ID, REF, ALT, QUAL, FILTER, INFO, FORMAT, SAMPLE…
/// Genotype is taken from the first sample column via its `GT` field.
///
/// NOTE: this remains the genotyping-array-shaped VCF reader (first sample,
/// biallelic-friendly). Multi-sample and `(chr,pos,ref,alt)` keying are the
/// subject of Phase 1.3; here we only move it onto the streaming trait.
pub struct VcfParser;

impl GenomeParser for VcfParser {
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

            // Meta-information lines (may carry the reference build).
            if trimmed.starts_with("##") {
                skipped_lines += 1;
                if build.is_none() {
                    build = detect_build_line(trimmed);
                }
                continue;
            }

            // Column header line, or any other stray comment line.
            if trimmed.starts_with('#') {
                skipped_lines += 1;
                continue;
            }

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

            let position: i64 = match pos_str.parse() {
                Ok(p) => p,
                Err(_) => {
                    skipped_lines += 1;
                    continue;
                }
            };

            // Synthesize a positional identifier when the variant has no rsID.
            let rsid_str = if rsid == "." {
                format!("chr{}:{}", chrom, position)
            } else {
                rsid.to_string()
            };

            // Locate the GT sub-field within FORMAT.
            let gt_index = format_field.split(':').position(|f| f == "GT");
            let genotype = match gt_index {
                Some(idx) => {
                    let sample_parts: Vec<&str> = sample_field.split(':').collect();
                    if idx < sample_parts.len() {
                        vcf_gt_to_genotype(sample_parts[idx], ref_allele, alt_allele)
                    } else {
                        skipped_lines += 1;
                        continue;
                    }
                }
                None => {
                    skipped_lines += 1;
                    continue;
                }
            };

            if genotype.is_empty() || genotype == ".." {
                skipped_lines += 1;
                continue;
            }

            sink.push(ParsedSnp {
                rsid: rsid_str,
                chromosome: chrom,
                position,
                genotype,
            })?;
            snp_count += 1;
        }

        Ok(ParseSummary {
            format: "vcf".to_string(),
            build,
            total_lines,
            skipped_lines,
            snp_count,
        })
    }
}

/// Parse a VCF file from an in-memory string.
///
/// Backward-compatible convenience wrapper around the streaming [`VcfParser`].
/// Prefer [`crate::parser::streaming::parse_path_streaming`] for large inputs —
/// whole-genome VCFs must not be loaded into a single `String`.
pub fn parse_vcf(content: &str) -> Result<ParseResult, AppError> {
    let mut sink = VecSink::default();
    let mut reader = Cursor::new(content.as_bytes());
    let summary = VcfParser.parse_streaming(&mut reader, &mut sink)?;
    Ok(ParseResult {
        format: summary.format,
        build: summary.build,
        total_lines: summary.total_lines,
        skipped_lines: summary.skipped_lines,
        snps: sink.snps,
    })
}

/// Convert VCF genotype notation (e.g., "0/1", "1|1") to an allele string.
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

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "##fileformat=VCFv4.2\n\
##reference=GRCh38\n\
#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tSAMPLE1\n\
1\t752566\trs3094315\tA\tG\t.\tPASS\t.\tGT\t0/1\n\
1\t776546\trs12124819\tA\tG\t.\tPASS\t.\tGT:DP\t1|1:35\n\
2\t100\t.\tC\tT\t.\tPASS\t.\tGT\t0/0\n\
3\t200\trs999\tG\tA\t.\tPASS\t.\tGT\t./.\n";

    #[test]
    fn streaming_parses_synthetic_vcf() {
        let mut sink = VecSink::default();
        let mut reader = Cursor::new(SAMPLE.as_bytes());
        let summary = VcfParser.parse_streaming(&mut reader, &mut sink).unwrap();

        // 4 data rows: the "./." missing genotype is skipped → 3 kept.
        assert_eq!(summary.snp_count, 3);
        assert_eq!(sink.snps.len(), 3);
        assert_eq!(summary.build.as_deref(), Some("GRCh38"));

        assert_eq!(sink.snps[0].genotype, "AG"); // 0/1 → REF+ALT
        assert_eq!(sink.snps[1].genotype, "GG"); // 1|1 → ALT+ALT
        // No-rsID variant gets a positional identifier.
        assert_eq!(sink.snps[2].rsid, "chr2:100");
        assert_eq!(sink.snps[2].genotype, "CC"); // 0/0 → REF+REF
    }

    #[test]
    fn wrapper_matches_streaming() {
        let result = parse_vcf(SAMPLE).unwrap();
        assert_eq!(result.format, "vcf");
        assert_eq!(result.snps.len(), 3);
    }
}
