pub mod ancestry;
pub mod csvarray;
pub mod streaming;
pub mod twentythree;
pub mod vcf;

use std::io::BufRead;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// A single parsed SNP / variant genotype from any file format.
///
/// For genotyping-array formats (23andMe, AncestryDNA) a row is fully described
/// by its rsID + genotype, and `ref_allele`/`alt_allele`/`sample` are `None`.
///
/// For VCF the variant *identity* is the locus tuple **`(chromosome, position,
/// ref_allele, alt_allele)`** — an rsID is optional and absent for the many
/// novel variants that never get one. Phase 1.3 therefore carries the REF/ALT
/// alleles explicitly and keys no-rsID variants positionally
/// (`chr{chr}:{pos}:{ref}:{alt}`, see [`variant_key`]). `sample` records which
/// sample column a genotype came from, so multi-sample VCFs no longer collapse
/// to just the first sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedSnp {
    pub rsid: String,
    pub chromosome: String,
    pub position: i64,
    pub genotype: String,
    /// Reference allele (VCF `REF`). `None` for array formats.
    #[serde(default)]
    pub ref_allele: Option<String>,
    /// Alternate allele the genotype is called against (VCF `ALT`). For a
    /// multiallelic site this is the full ALT list observed at the site.
    /// `None` for array formats.
    #[serde(default)]
    pub alt_allele: Option<String>,
    /// Originating sample name (from the VCF `#CHROM` header). `None` for
    /// single-column array formats.
    #[serde(default)]
    pub sample: Option<String>,
}

/// Canonical variant key for a locus: `chr{chromosome}:{position}:{ref}:{alt}`.
///
/// This is the `(chr,pos,ref,alt)` identity used to distinguish variants that
/// share a position (e.g. multiallelic sites, or a SNP and an indel at the same
/// coordinate) and to give novel, rsID-less variants a stable, unique handle.
/// The chromosome is normalized (no `chr` prefix, upper-cased) by the caller
/// before this is built.
pub fn variant_key(chromosome: &str, position: i64, ref_allele: &str, alt_allele: &str) -> String {
    format!("chr{}:{}:{}:{}", chromosome, position, ref_allele, alt_allele)
}

/// Result of parsing a raw DNA data file.
///
/// Retained for the in-memory `parse_*(&str)` convenience wrappers and callers
/// that want the whole result at once. New ingestion paths should prefer the
/// streaming [`GenomeParser`] trait, which never materializes the full genome.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseResult {
    pub snps: Vec<ParsedSnp>,
    pub format: String,
    pub build: Option<String>,
    pub total_lines: usize,
    pub skipped_lines: usize,
}

/// Summary statistics produced by a streaming parse.
///
/// Unlike [`ParseResult`], this carries only counts (not the SNPs themselves),
/// because in a streaming parse each SNP is handed to a [`SnpSink`] as soon as
/// it is read and is never accumulated by the parser.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseSummary {
    pub format: String,
    pub build: Option<String>,
    pub total_lines: usize,
    pub skipped_lines: usize,
    pub snp_count: usize,
}

/// A consumer of parsed SNPs.
///
/// Implementors decide what to do with each SNP as it is produced — collect it
/// into a `Vec`, stream it to the database in bounded batches, count it, etc.
/// This is the mechanism that lets parsers stay O(1) in memory regardless of
/// input size: a whole-genome VCF of tens of gigabytes flows through the parser
/// one record at a time and out to the sink, never held in RAM all at once.
pub trait SnpSink {
    fn push(&mut self, snp: ParsedSnp) -> Result<(), AppError>;
}

/// A genome-file parser that consumes a buffered reader incrementally.
///
/// The contract: `parse_streaming` must read its input in bounded chunks (line
/// by line for the text formats here) and emit every variant to `sink` as soon
/// as it is parsed. Implementors must **not** read the entire input into memory.
pub trait GenomeParser {
    fn parse_streaming(
        &self,
        reader: &mut dyn BufRead,
        sink: &mut dyn SnpSink,
    ) -> Result<ParseSummary, AppError>;
}

/// Simple in-memory sink that collects every SNP into a `Vec`.
///
/// Used by tests and by the backward-compatible `parse_*(&str)` wrappers. Not
/// suitable for whole-genome-scale files (that is what the DB batch sink in
/// `commands::import` is for) — it is the explicit "materialize everything"
/// choice a caller opts into.
#[derive(Debug, Default)]
pub struct VecSink {
    pub snps: Vec<ParsedSnp>,
}

impl SnpSink for VecSink {
    fn push(&mut self, snp: ParsedSnp) -> Result<(), AppError> {
        self.snps.push(snp);
        Ok(())
    }
}

/// A sink that counts SNPs without retaining them. Useful for validation and
/// dry-run "how many variants would this import?" passes.
#[derive(Debug, Default)]
pub struct CountingSink {
    pub count: usize,
}

impl SnpSink for CountingSink {
    fn push(&mut self, _snp: ParsedSnp) -> Result<(), AppError> {
        self.count += 1;
        Ok(())
    }
}

/// Return the streaming parser for a detected format string, if supported.
///
/// Accepts the same format identifiers produced by [`detect_format`].
pub fn parser_for_format(format: &str) -> Option<Box<dyn GenomeParser>> {
    match format {
        "23andme_v5" | "23andme_v3" => Some(Box::new(twentythree::TwentyThreeParser)),
        "ancestry" => Some(Box::new(ancestry::AncestryParser)),
        // LivingDNA, tellmeGen and Genes for Good all export the same
        // 23andMe-style tab-delimited `rsid<TAB>chromosome<TAB>position<TAB>genotype`
        // layout, so they share the TwentyThreeParser — only their provenance
        // label (from `detect_format`) differs.
        "livingdna" | "tellmegen" | "genesforgood" => {
            Some(Box::new(twentythree::TwentyThreeParser))
        }
        "vcf" => Some(Box::new(vcf::VcfParser)),
        "myheritage" | "ftdna" => Some(Box::new(csvarray::CsvArrayParser)),
        _ => None,
    }
}

/// Detect the file format by inspecting header content.
/// Returns one of: "23andme_v5", "23andme_v3", "ancestry", "vcf",
/// "myheritage", "ftdna", "livingdna", "tellmegen", "genesforgood", "unknown".
pub fn detect_format(content: &str) -> String {
    // Take the first few KB for detection
    let header: String = content.chars().take(4096).collect();

    if header.starts_with("##fileformat=VCF") {
        return "vcf".to_string();
    }

    // AncestryDNA files contain "AncestryDNA" in the header
    if header.contains("AncestryDNA") {
        return "ancestry".to_string();
    }

    // MyHeritage exports announce themselves in a comment header and use the
    // quoted-CSV `RSID,CHROMOSOME,POSITION,RESULT` layout.
    if header.contains("MyHeritage") {
        return "myheritage".to_string();
    }

    // FamilyTreeDNA (FTDNA) exports have no comment header — they begin directly
    // with the quoted-CSV column header row. Match it explicitly (comma-joined,
    // uppercase, quotes stripped) so we don't collide with the tab formats.
    for line in header.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if csvarray::is_column_header(line) {
            return "ftdna".to_string();
        }
        break; // only inspect the first meaningful line
    }

    // ── Consumer array formats that share the 23andMe tab-delimited shape ──
    //
    // LivingDNA, tellmeGen and Genes for Good all export the exact
    // `rsid<TAB>chromosome<TAB>position<TAB>genotype` layout 23andMe uses; they
    // differ only by a provider token in their comment header. Detect them by
    // that token *before* the generic 23andMe block so a Genes-for-Good
    // `*_23andMe.txt` export (whose header literally mentions "23andMe") is still
    // attributed to Genes for Good. All three route to the TwentyThreeParser.
    //
    // A headerless export of any of these (e.g. some tellmeGen dumps carry only
    // the bare `# rsid chromosome position genotype` line) simply falls through
    // to the generic 23andMe detection below and still parses correctly — only
    // the provenance label is the generic one in that case.
    if header.contains("Living DNA") || header.contains("LivingDNA") {
        return "livingdna".to_string();
    }
    if header.contains("Genes for Good")
        || header.contains("Genes For Good")
        || header.contains("genesforgood")
        || header.contains("GenesForGood")
    {
        return "genesforgood".to_string();
    }
    if header.contains("tellmeGen") || header.contains("tellmegen") || header.contains("TellMeGen")
    {
        return "tellmegen".to_string();
    }

    // 23andMe v5 files typically have a header line with column names after comment lines
    if header.contains("23andMe") || header.contains("# rsid") {
        // v5 has the explicit header line "# rsid\tchromosome\tposition\tgenotype"
        // v3 may lack that or have different formatting
        if header.contains("# rsid\tchromosome\tposition\tgenotype")
            || header.contains("# This data has been generated by 23andMe")
        {
            // Distinguish v5 vs v3 by looking for specific v5 markers
            // v5 files often mention "build 37" and have more SNPs
            if header.contains("v5") || header.contains("build 37") || header.contains("# rsid\t") {
                return "23andme_v5".to_string();
            }
        }

        return "23andme_v3".to_string();
    }

    // Fallback: if lines look like tab-separated with rsid, chr, pos, genotype
    for line in header.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 4 && parts[0].starts_with("rs") {
            return "23andme_v3".to_string();
        }
        if parts.len() >= 5 && parts[0].starts_with("rs") {
            return "ancestry".to_string();
        }
        break;
    }

    "unknown".to_string()
}

#[cfg(test)]
mod detect_tests {
    use super::detect_format;

    #[test]
    fn detects_myheritage_and_ftdna_without_regressing_others() {
        // MyHeritage: comment header + quoted CSV.
        let mh = "# MyHeritage DNA raw data.\nRSID,CHROMOSOME,POSITION,RESULT\n\"rs1\",\"1\",\"100\",\"AA\"\n";
        assert_eq!(detect_format(mh), "myheritage");

        // FTDNA: no comments, bare quoted-CSV header.
        let ftdna = "RSID,CHROMOSOME,POSITION,RESULT\n\"rs1\",\"1\",\"100\",\"AA\"\n";
        assert_eq!(detect_format(ftdna), "ftdna");

        // Existing formats must still route correctly (no regression).
        assert_eq!(detect_format("##fileformat=VCFv4.2\n#CHROM\tPOS\n"), "vcf");
        assert_eq!(
            detect_format("#AncestryDNA raw data download\nrsid\tchromosome\tposition\tallele1\tallele2\n"),
            "ancestry"
        );
        assert_eq!(
            detect_format("# This data has been generated by 23andMe\n# rsid\tchromosome\tposition\tgenotype\n"),
            "23andme_v5"
        );
        assert_eq!(detect_format("random junk\n"), "unknown");
    }

    #[test]
    fn detects_livingdna_tellmegen_genesforgood_by_provider_token() {
        // LivingDNA: real exports carry this comment banner then the 23andMe
        // tab layout.
        let livingdna = "# Living DNA customer genotype data download file version: 1.0.1\n\
# rsid\tchromosome\tposition\tgenotype\n\
rs4477212\t1\t82154\tAA\n";
        assert_eq!(detect_format(livingdna), "livingdna");

        // Genes for Good ships a `*_23andMe.txt`; its header mentions "23andMe"
        // but must still be attributed to Genes for Good (provider token wins).
        let gfg = "# Genes for Good genotype data (23andMe format)\n\
# rsid\tchromosome\tposition\tgenotype\n\
rs11240777\t1\t798959\tGG\n";
        assert_eq!(detect_format(gfg), "genesforgood");

        // tellmeGen: named comment header.
        let tmg = "# tellmeGen raw data export\n\
# rsid\tchromosome\tposition\tgenotype\n\
rs991757223\t1\t100177980\tDD\n";
        assert_eq!(detect_format(tmg), "tellmegen");

        // A *headerless* tellmeGen-shape file (only the bare column comment)
        // still parses — it just falls back to the generic 23andMe label.
        let bare = "# rsid\tchromosome\tposition\tgenotype\n\
rs991757223\t1\t100177980\tDD\n";
        assert_eq!(detect_format(bare), "23andme_v5");

        // None of the new tokens may perturb the previously-supported formats.
        assert_eq!(detect_format("##fileformat=VCFv4.2\n#CHROM\tPOS\n"), "vcf");
        assert_eq!(
            detect_format("# MyHeritage DNA raw data.\nRSID,CHROMOSOME,POSITION,RESULT\n"),
            "myheritage"
        );
    }
}
