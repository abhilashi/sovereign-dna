use std::io::{BufRead, Cursor};

use crate::error::AppError;

use super::streaming::detect_build_line;
use super::{GenomeParser, ParseResult, ParseSummary, ParsedSnp, SnpSink, VecSink};

/// Streaming parser for the quoted-CSV consumer genotyping exports used by
/// **MyHeritage** and **FamilyTreeDNA (FTDNA)**.
///
/// Both share the same tidy shape — a `RSID,CHROMOSOME,POSITION,RESULT` header
/// followed by comma-separated, double-quoted rows:
///
/// ```text
/// # MyHeritage DNA raw data.           (MyHeritage only; FTDNA has no comments)
/// RSID,CHROMOSOME,POSITION,RESULT
/// "rs4477212","1","82154","AA"
/// "rs3094315","1","752566","AG"
/// ```
///
/// Like the other array formats this is rsID-centric (no REF/ALT or per-sample
/// dimension), so emitted [`ParsedSnp`]s leave those fields `None`. Parsing is
/// line-at-a-time and O(1) in memory, consistent with the streaming trait.
///
/// The alleles here never contain embedded commas, so a simple split-and-dequote
/// is sufficient and keeps `#`-comment build detection (which a CSV library that
/// silently drops comment lines would lose).
pub struct CsvArrayParser;

/// Strip surrounding double quotes and whitespace from a CSV field.
fn dequote(field: &str) -> &str {
    field.trim().trim_matches('"').trim()
}

/// Is this line the `RSID,CHROMOSOME,POSITION,RESULT` column header (in any
/// case, with or without quotes)? Also used by `detect_format` to recognize
/// header-only FTDNA exports.
pub(crate) fn is_column_header(line: &str) -> bool {
    let cols: Vec<String> = line
        .split(',')
        .map(|c| dequote(c).to_ascii_uppercase())
        .collect();
    cols.len() >= 4
        && cols[0] == "RSID"
        && cols[1] == "CHROMOSOME"
        && cols[2] == "POSITION"
        && (cols[3] == "RESULT" || cols[3] == "GENOTYPE")
}

impl GenomeParser for CsvArrayParser {
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

            // Comment/metadata lines (MyHeritage) carry the reference build.
            if trimmed.starts_with('#') {
                skipped_lines += 1;
                if build.is_none() {
                    build = detect_build_line(trimmed);
                }
                continue;
            }

            // The column header row itself.
            if is_column_header(trimmed) {
                skipped_lines += 1;
                continue;
            }

            let mut cols = trimmed.split(',');
            let rsid = dequote(cols.next().unwrap_or(""));
            let chromosome = dequote(cols.next().unwrap_or(""));
            let position_str = dequote(cols.next().unwrap_or(""));
            let genotype = dequote(cols.next().unwrap_or(""));

            if rsid.is_empty()
                || chromosome.is_empty()
                || position_str.is_empty()
                || genotype.is_empty()
            {
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

            // Skip no-calls / deletions (MyHeritage/FTDNA use "--" and "00").
            if genotype == "--" || genotype == "00" {
                skipped_lines += 1;
                continue;
            }

            let chrom = chromosome
                .strip_prefix("chr")
                .unwrap_or(chromosome)
                .to_uppercase();

            sink.push(ParsedSnp {
                rsid: rsid.to_string(),
                chromosome: chrom,
                position,
                genotype: genotype.to_uppercase(),
                // rsID-centric array format: no REF/ALT or sample dimension.
                ref_allele: None,
                alt_allele: None,
                sample: None,
            })?;
            snp_count += 1;
        }

        Ok(ParseSummary {
            format: "csvarray".to_string(),
            build,
            total_lines,
            skipped_lines,
            snp_count,
        })
    }
}

/// Parse a MyHeritage/FTDNA CSV file from an in-memory string.
///
/// Backward-compatible convenience wrapper around the streaming
/// [`CsvArrayParser`]; collects into a `Vec` via [`VecSink`], so use it only for
/// known-small inputs. Large files should go through
/// [`crate::parser::streaming::parse_path_streaming`].
pub fn parse_csv_array(content: &str) -> Result<ParseResult, AppError> {
    let mut sink = VecSink::default();
    let mut reader = Cursor::new(content.as_bytes());
    let summary = CsvArrayParser.parse_streaming(&mut reader, &mut sink)?;
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

    const MYHERITAGE: &str = "# MyHeritage DNA raw data.\n\
# For more information visit: https://www.myheritage.com/dna\n\
# reference build 37\n\
RSID,CHROMOSOME,POSITION,RESULT\n\
\"rs4477212\",\"1\",\"82154\",\"AA\"\n\
\"rs3094315\",\"1\",\"752566\",\"AG\"\n\
\"rs3131972\",\"1\",\"752721\",\"--\"\n\
\"rs12124819\",\"1\",\"776546\",\"GG\"\n";

    // FTDNA: no comment header, bare column header, quoted rows.
    const FTDNA: &str = "RSID,CHROMOSOME,POSITION,RESULT\n\
\"rs4477212\",\"1\",\"82154\",\"AA\"\n\
\"rs3094315\",\"1\",\"752566\",\"AG\"\n\
\"i5000001\",\"MT\",\"73\",\"G\"\n";

    #[test]
    fn parses_myheritage_with_comments_and_build() {
        let mut sink = VecSink::default();
        let mut reader = Cursor::new(MYHERITAGE.as_bytes());
        let summary = CsvArrayParser
            .parse_streaming(&mut reader, &mut sink)
            .unwrap();

        // 4 data rows, one no-call ("--") → 3 kept.
        assert_eq!(summary.snp_count, 3);
        assert_eq!(sink.snps.len(), 3);
        assert_eq!(summary.build.as_deref(), Some("GRCh37"));

        assert_eq!(sink.snps[0].rsid, "rs4477212");
        assert_eq!(sink.snps[0].chromosome, "1");
        assert_eq!(sink.snps[0].position, 82154);
        assert_eq!(sink.snps[0].genotype, "AA");
        // Array format → no REF/ALT/sample.
        assert!(sink.snps[0].ref_allele.is_none());
        assert!(sink.snps[0].sample.is_none());
    }

    #[test]
    fn parses_ftdna_without_comment_header() {
        let mut sink = VecSink::default();
        let mut reader = Cursor::new(FTDNA.as_bytes());
        let summary = CsvArrayParser
            .parse_streaming(&mut reader, &mut sink)
            .unwrap();

        assert_eq!(summary.snp_count, 3);
        assert_eq!(sink.snps[2].chromosome, "MT");
        assert_eq!(sink.snps[2].genotype, "G");
    }

    #[test]
    fn is_column_header_detects_quoted_and_unquoted() {
        assert!(is_column_header("RSID,CHROMOSOME,POSITION,RESULT"));
        assert!(is_column_header(
            "\"RSID\",\"CHROMOSOME\",\"POSITION\",\"RESULT\""
        ));
        assert!(is_column_header("rsid,chromosome,position,genotype"));
        assert!(!is_column_header("\"rs4477212\",\"1\",\"82154\",\"AA\""));
    }

    #[test]
    fn wrapper_matches_streaming() {
        let result = parse_csv_array(FTDNA).unwrap();
        assert_eq!(result.format, "csvarray");
        assert_eq!(result.snps.len(), 3);
    }
}
