use std::io::{BufRead, Cursor};

use crate::error::AppError;

use super::streaming::detect_build_line;
use super::{variant_key, GenomeParser, ParseResult, ParseSummary, ParsedSnp, SnpSink, VecSink};

/// The two symbolic "non-reference" alleles a gVCF uses to represent "any other
/// (uncalled) allele". GATK writes `<NON_REF>`; DeepVariant / bcftools write
/// `<*>`. Neither is a concrete variant — a genotype that lands on one carries
/// no allele information and is treated as a no-call.
const NON_REF_TOKENS: [&str; 2] = ["<NON_REF>", "<*>"];

fn is_non_ref(allele: &str) -> bool {
    NON_REF_TOKENS.contains(&allele)
}

/// Streaming parser for **gVCF** (genomic VCF) files.
///
/// A gVCF is a VCF profile emitted by variant callers (GATK `HaplotypeCaller
/// -ERC GVCF`, DeepVariant, …) that additionally records the **reference
/// blocks** between variants, so every position of the genome is accounted for.
/// It differs from a plain VCF in two ways this parser must handle correctly:
///
/// 1. **The symbolic `<NON_REF>` / `<*>` allele.** Every ALT list ends with one
///    of these placeholders. A naive VCF parser would try to resolve a genotype
///    index pointing at it into a base and emit garbage. Here it is stripped
///    from the effective ALT list and any genotype call landing on it is a
///    no-call.
/// 2. **Reference blocks.** Records whose *only* ALT is the symbolic allele
///    (typically carrying `END=` in INFO and a `0/0` genotype) are hom-ref
///    spans, **not** variants. They are the bulk of a gVCF and are skipped, so
///    ingestion yields the actual called variants — not one row per base.
///
/// Records that carry at least one real ALT allele are parsed like a normal VCF
/// record (multi-sample aware, `(chr,pos,ref,alt)`-keyed when no rsID), decoding
/// genotype indices against the full ALT list (including the symbolic tail) so
/// index positions stay correct.
pub struct GvcfParser;

impl GenomeParser for GvcfParser {
    fn parse_streaming(
        &self,
        reader: &mut dyn BufRead,
        sink: &mut dyn SnpSink,
    ) -> Result<ParseSummary, AppError> {
        let mut total_lines: usize = 0;
        let mut skipped_lines: usize = 0;
        let mut snp_count: usize = 0;
        let mut build: Option<String> = None;
        let mut samples: Vec<String> = Vec::new();

        let mut line = String::new();
        loop {
            line.clear();
            let bytes = reader
                .read_line(&mut line)
                .map_err(|e| AppError::Io(e.to_string()))?;
            if bytes == 0 {
                break;
            }
            total_lines += 1;
            let trimmed = line.trim();

            if trimmed.is_empty() {
                skipped_lines += 1;
                continue;
            }

            if trimmed.starts_with("##") {
                skipped_lines += 1;
                if build.is_none() {
                    build = detect_build_line(trimmed);
                }
                continue;
            }

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
            let id = fields[2];
            let ref_allele = fields[3];
            let alt_field = fields[4];
            let format_field = fields[8];
            let sample_fields = &fields[9..];

            let position: i64 = match fields[1].parse() {
                Ok(p) => p,
                Err(_) => {
                    skipped_lines += 1;
                    continue;
                }
            };

            // The full ALT list as written (genotype indices are relative to it).
            let alt_list: Vec<&str> = alt_field.split(',').collect();
            // The real (concrete) ALT alleles, with the symbolic tail removed.
            let real_alts: Vec<&str> = alt_list
                .iter()
                .copied()
                .filter(|a| !is_non_ref(a) && *a != ".")
                .collect();

            // Reference block (no concrete ALT) — a hom-ref span, not a variant.
            if real_alts.is_empty() {
                skipped_lines += 1;
                continue;
            }

            // Canonical ALT string for storage/keying uses only the real alleles.
            let canonical_alt = real_alts.join(",");
            let key = if id == "." {
                variant_key(&chrom, position, ref_allele, &canonical_alt)
            } else {
                id.to_string()
            };

            let gt_index = match format_field.split(':').position(|f| f == "GT") {
                Some(idx) => idx,
                None => {
                    skipped_lines += 1;
                    continue;
                }
            };

            let mut emitted_any = false;
            for (i, sample_field) in sample_fields.iter().enumerate() {
                let sample_parts: Vec<&str> = sample_field.split(':').collect();
                let genotype = if gt_index < sample_parts.len() {
                    gvcf_gt_to_genotype(sample_parts[gt_index], ref_allele, &alt_list)
                } else {
                    None
                };

                let genotype = match genotype {
                    Some(g) if !g.is_empty() => g,
                    _ => continue, // missing / symbolic-only call for this sample
                };

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
                    alt_allele: Some(canonical_alt.clone()),
                    sample: Some(sample_name),
                })?;
                snp_count += 1;
                emitted_any = true;
            }

            if !emitted_any {
                skipped_lines += 1;
            }
        }

        Ok(ParseSummary {
            format: "gvcf".to_string(),
            build,
            total_lines,
            skipped_lines,
            snp_count,
        })
    }
}

/// Decode a gVCF genotype against the **full** ALT list (symbolic tail included,
/// so indices stay aligned). Returns `None` for a call that carries no concrete
/// allele information: a missing genotype (`.`), or a call whose allele index
/// points at the symbolic `<NON_REF>`/`<*>` placeholder.
fn gvcf_gt_to_genotype(gt: &str, ref_allele: &str, alt_list: &[&str]) -> Option<String> {
    let separator = if gt.contains('|') { '|' } else { '/' };
    let alleles: Vec<&str> = gt.split(separator).collect();
    if alleles.len() != 2 {
        return None;
    }

    let mut result = String::with_capacity(2);
    for allele_str in &alleles {
        match *allele_str {
            "." => return None,
            "0" => {
                result.push(ref_allele.chars().next()?);
            }
            _ => {
                let idx: usize = allele_str.parse().ok()?;
                // A malformed genotype like `00`/`000` parses to index 0 (which
                // the `"0"` arm didn't catch); `idx.checked_sub(1)` avoids an
                // unsigned-underflow panic and treats it as a no-call.
                let alt = *alt_list.get(idx.checked_sub(1)?)?;
                if is_non_ref(alt) {
                    // Genotype lands on the "any other allele" placeholder —
                    // no concrete base to record.
                    return None;
                }
                result.push(alt.chars().next()?);
            }
        }
    }
    Some(result.to_uppercase())
}

/// Parse a gVCF from an in-memory string (backward-compatible convenience
/// wrapper; prefer the streaming path for whole-genome inputs).
pub fn parse_gvcf(content: &str) -> Result<ParseResult, AppError> {
    let mut sink = VecSink::default();
    let mut reader = Cursor::new(content.as_bytes());
    let summary = GvcfParser.parse_streaming(&mut reader, &mut sink)?;
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

    // A GATK-style gVCF: a reference block (END= + <NON_REF> only), a real SNP
    // with the trailing <NON_REF>, and a genotype landing on <NON_REF>.
    const GVCF: &str = "##fileformat=VCFv4.2\n\
##reference=GRCh38\n\
##ALT=<ID=NON_REF,Description=\"Represents any possible alternative allele\">\n\
##GVCFBlock0-1=minGQ=0(inclusive),maxGQ=1(exclusive)\n\
#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tSAMPLE1\n\
1\t100\t.\tA\t<NON_REF>\t.\t.\tEND=150\tGT:DP\t0/0:30\n\
1\t200\trs123\tC\tT,<NON_REF>\t.\tPASS\t.\tGT:DP\t0/1:40\n\
1\t300\t.\tG\t<NON_REF>\t.\t.\tEND=305\tGT\t0/0\n\
2\t400\t.\tA\tG,<NON_REF>\t.\tPASS\t.\tGT\t2/2\n";

    #[test]
    fn skips_reference_blocks_and_decodes_real_variants() {
        let mut sink = VecSink::default();
        let mut reader = Cursor::new(GVCF.as_bytes());
        let summary = GvcfParser.parse_streaming(&mut reader, &mut sink).unwrap();

        // Two ref blocks (pos 100, 300) skipped; pos 400 genotype 2/2 lands on
        // <NON_REF> (allele index 2) -> no-call skipped. Only the pos-200 SNP
        // survives.
        assert_eq!(summary.snp_count, 1);
        assert_eq!(sink.snps.len(), 1);
        assert_eq!(summary.build.as_deref(), Some("GRCh38"));

        let snp = &sink.snps[0];
        assert_eq!(snp.position, 200);
        assert_eq!(snp.rsid, "rs123");
        assert_eq!(snp.genotype, "CT"); // 0/1 -> REF(C)+ALT(T)
        // The symbolic <NON_REF> tail is stripped from the stored ALT.
        assert_eq!(snp.ref_allele.as_deref(), Some("C"));
        assert_eq!(snp.alt_allele.as_deref(), Some("T"));
        assert_eq!(snp.sample.as_deref(), Some("SAMPLE1"));
    }

    #[test]
    fn novel_gvcf_variant_keyed_without_symbolic_allele() {
        // A no-rsID SNP: the (chr,pos,ref,alt) key must use only the real ALT,
        // never the <NON_REF> placeholder.
        let vcf = "##fileformat=VCFv4.2\n\
#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tS1\n\
5\t900\t.\tA\tT,<NON_REF>\t.\tPASS\t.\tGT\t1/1\n";
        let mut sink = VecSink::default();
        let mut reader = Cursor::new(vcf.as_bytes());
        GvcfParser.parse_streaming(&mut reader, &mut sink).unwrap();

        assert_eq!(sink.snps.len(), 1);
        assert_eq!(sink.snps[0].genotype, "TT");
        assert_eq!(sink.snps[0].rsid, "chr5:900:A:T");
    }

    #[test]
    fn handles_star_symbolic_allele_variant() {
        // DeepVariant / bcftools use `<*>` for the same placeholder.
        let vcf = "##fileformat=VCFv4.2\n\
#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tS1\n\
7\t10\t.\tG\tA,<*>\t.\tPASS\t.\tGT\t0/1\n\
7\t20\t.\tG\t<*>\t.\t.\tEND=25\tGT\t0/0\n";
        let mut sink = VecSink::default();
        let mut reader = Cursor::new(vcf.as_bytes());
        let summary = GvcfParser.parse_streaming(&mut reader, &mut sink).unwrap();

        // The `<*>`-only ref block is skipped; the real SNP survives.
        assert_eq!(summary.snp_count, 1);
        assert_eq!(sink.snps[0].genotype, "GA");
        assert_eq!(sink.snps[0].alt_allele.as_deref(), Some("A"));
    }

    #[test]
    fn wrapper_matches_streaming() {
        let result = parse_gvcf(GVCF).unwrap();
        assert_eq!(result.format, "gvcf");
        assert_eq!(result.snps.len(), 1);
    }
}
