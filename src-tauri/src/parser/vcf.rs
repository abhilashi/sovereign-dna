use std::io::{BufRead, Cursor};

use crate::error::AppError;

use super::streaming::detect_build_line;
use super::{variant_key, GenomeParser, ParseResult, ParseSummary, ParsedSnp, SnpSink, VecSink};

/// Streaming parser for VCF (Variant Call Format) files.
///
/// Meta-information lines start with `##`; the header line starts with `#CHROM`.
/// Data columns: CHROM, POS, ID, REF, ALT, QUAL, FILTER, INFO, FORMAT, SAMPLE…
///
/// **Phase 1.3 — multi-sample + `(chr,pos,ref,alt)` keying:**
/// - The `#CHROM` header is parsed to recover every sample name.
/// - Each data record emits **one [`ParsedSnp`] per sample** (not just the
///   first column), tagged with its `sample` name, so multi-sample VCFs are no
///   longer collapsed to a single individual.
/// - Every emitted genotype carries the site's `ref_allele`/`alt_allele`, and
///   variants without an rsID are keyed by the locus tuple
///   `chr{chr}:{pos}:{ref}:{alt}` (see [`variant_key`]) instead of position
///   alone — so novel variants and co-located multiallelic/indel sites stay
///   distinct.
/// - A per-sample missing genotype (`./.`) is skipped for that sample only; the
///   record still contributes its other samples' calls.
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
        // Sample names recovered from the `#CHROM` header (columns 9..).
        let mut samples: Vec<String> = Vec::new();

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

            // Column header line: `#CHROM POS ID REF ALT QUAL FILTER INFO FORMAT
            // SAMPLE1 SAMPLE2 …`. Capture the sample names so each genotype can
            // be attributed to its source individual.
            if trimmed.starts_with('#') {
                let cols: Vec<&str> = trimmed.split('\t').collect();
                if cols.first() == Some(&"#CHROM") && cols.len() > 9 {
                    samples = cols[9..].iter().map(|s| s.to_string()).collect();
                }
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
            let id = fields[2];
            let ref_allele = fields[3];
            let alt_allele = fields[4];
            let format_field = fields[8];
            let sample_fields = &fields[9..];

            let position: i64 = match pos_str.parse() {
                Ok(p) => p,
                Err(_) => {
                    skipped_lines += 1;
                    continue;
                }
            };

            // Variant identity: use the rsID when present, otherwise the
            // canonical (chr,pos,ref,alt) locus key so novel and co-located
            // (multiallelic / indel) variants remain distinct.
            let key = if id == "." {
                variant_key(&chrom, position, ref_allele, alt_allele)
            } else {
                id.to_string()
            };

            // Locate the GT sub-field within FORMAT once for the whole record.
            let gt_index = match format_field.split(':').position(|f| f == "GT") {
                Some(idx) => idx,
                None => {
                    skipped_lines += 1;
                    continue;
                }
            };

            // Emit one genotype per sample column. A per-sample missing call is
            // skipped for that sample only; the record still yields the rest.
            let mut emitted_any = false;
            for (i, sample_field) in sample_fields.iter().enumerate() {
                let sample_parts: Vec<&str> = sample_field.split(':').collect();
                let genotype = if gt_index < sample_parts.len() {
                    vcf_gt_to_genotype(sample_parts[gt_index], ref_allele, alt_allele)
                } else {
                    String::new()
                };

                if genotype.is_empty() || genotype == ".." {
                    continue; // missing/no-call for this sample
                }

                // Prefer the declared sample name; fall back to a positional
                // label for headerless / malformed inputs.
                let sample_name = samples
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| format!("SAMPLE{}", i + 1));

                sink.push(ParsedSnp {
                    rsid: key.clone(),
                    chromosome: chrom.clone(),
                    position,
                    genotype,
                    ref_allele: Some(ref_allele.to_string()),
                    alt_allele: Some(alt_allele.to_string()),
                    sample: Some(sample_name),
                })?;
                snp_count += 1;
                emitted_any = true;
            }

            // Only count the whole line as skipped if no sample produced a call.
            if !emitted_any {
                skipped_lines += 1;
            }
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

        // 4 data rows, single sample: the "./." missing genotype is skipped → 3 kept.
        assert_eq!(summary.snp_count, 3);
        assert_eq!(sink.snps.len(), 3);
        assert_eq!(summary.build.as_deref(), Some("GRCh38"));

        assert_eq!(sink.snps[0].genotype, "AG"); // 0/1 → REF+ALT
        assert_eq!(sink.snps[1].genotype, "GG"); // 1|1 → ALT+ALT
        assert_eq!(sink.snps[2].genotype, "CC"); // 0/0 → REF+REF

        // Phase 1.3: REF/ALT + sample now populated; no-rsID variant keyed by
        // the full (chr,pos,ref,alt) locus tuple.
        assert_eq!(sink.snps[0].ref_allele.as_deref(), Some("A"));
        assert_eq!(sink.snps[0].alt_allele.as_deref(), Some("G"));
        assert_eq!(sink.snps[0].sample.as_deref(), Some("SAMPLE1"));
        assert_eq!(sink.snps[2].rsid, "chr2:100:C:T");
    }

    #[test]
    fn wrapper_matches_streaming() {
        let result = parse_vcf(SAMPLE).unwrap();
        assert_eq!(result.format, "vcf");
        assert_eq!(result.snps.len(), 3);
    }

    const MULTISAMPLE: &str = "##fileformat=VCFv4.2\n\
##reference=GRCh37\n\
#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tNA001\tNA002\n\
1\t100\trs1\tA\tG\t.\tPASS\t.\tGT\t0/1\t1/1\n\
1\t200\t.\tC\tT\t.\tPASS\t.\tGT\t0/0\t./.\n";

    #[test]
    fn multi_sample_emits_a_row_per_sample() {
        let mut sink = VecSink::default();
        let mut reader = Cursor::new(MULTISAMPLE.as_bytes());
        let summary = VcfParser.parse_streaming(&mut reader, &mut sink).unwrap();

        // Row 1: 2 samples → 2 calls (AG, GG). Row 2: NA001 0/0 → CC, NA002 ./.
        // skipped → 1 call. Total = 3.
        assert_eq!(summary.snp_count, 3);
        assert_eq!(sink.snps.len(), 3);
        assert_eq!(summary.build.as_deref(), Some("GRCh37"));

        // First record attributed to both samples.
        assert_eq!(sink.snps[0].sample.as_deref(), Some("NA001"));
        assert_eq!(sink.snps[0].genotype, "AG"); // 0/1
        assert_eq!(sink.snps[1].sample.as_deref(), Some("NA002"));
        assert_eq!(sink.snps[1].genotype, "GG"); // 1/1

        // Second record: only NA001 has a call; keyed by (chr,pos,ref,alt).
        assert_eq!(sink.snps[2].sample.as_deref(), Some("NA001"));
        assert_eq!(sink.snps[2].rsid, "chr1:200:C:T");
        assert_eq!(sink.snps[2].genotype, "CC");
        assert_eq!(sink.snps[2].ref_allele.as_deref(), Some("C"));
        assert_eq!(sink.snps[2].alt_allele.as_deref(), Some("T"));
    }

    #[test]
    fn multiallelic_site_resolves_alt_and_keys_full_alt_list() {
        // ALT has two alleles; genotype 1/2 picks one of each. The variant key
        // preserves the full ALT list so the multiallelic site is unambiguous.
        let vcf = "##fileformat=VCFv4.2\n\
#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tS1\n\
5\t300\t.\tA\tG,T\t.\tPASS\t.\tGT\t1/2\n";
        let mut sink = VecSink::default();
        let mut reader = Cursor::new(vcf.as_bytes());
        VcfParser.parse_streaming(&mut reader, &mut sink).unwrap();

        assert_eq!(sink.snps.len(), 1);
        assert_eq!(sink.snps[0].genotype, "GT"); // allele 1 (G) + allele 2 (T)
        assert_eq!(sink.snps[0].alt_allele.as_deref(), Some("G,T"));
        assert_eq!(sink.snps[0].rsid, "chr5:300:A:G,T");
    }

    #[test]
    fn variant_key_is_chr_pos_ref_alt() {
        assert_eq!(super::variant_key("7", 1000, "A", "T"), "chr7:1000:A:T");
    }
}
