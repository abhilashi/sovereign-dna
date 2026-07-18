//! Import provenance + UX metadata (Phase 1.9).
//!
//! When a genome is imported we want to tell the user *exactly* what we detected
//! and flag anything that could make downstream annotation wrong or incomplete —
//! rather than silently ingesting. This module is the pure, testable core that:
//!
//! - maps a detected format id to a human **source label** (which service /
//!   file type the data came from), for display and per-genome provenance
//!   storage;
//! - derives **quality** signals (skip rate) and turns detection facts into
//!   plain-language **warnings**: unknown reference build, a build that differs
//!   from the one our reference annotations use (with a pointer to the Phase 1.6
//!   liftover), and an unusually high share of skipped lines.
//!
//! The reference build our bundled annotations are aligned to, and the default
//! target the liftover harmonizes to.
pub const REFERENCE_BUILD: &str = "GRCh38";

/// Human-readable source label for a detected format id (see
/// [`super::detect_format`]). Used both for display and for the per-genome
/// `source_label` provenance column.
pub fn source_label(format: &str) -> &'static str {
    match format {
        "23andme_v5" => "23andMe (v5 array)",
        "23andme_v3" => "23andMe (v3 array)",
        "ancestry" => "AncestryDNA (array)",
        "myheritage" => "MyHeritage (array)",
        "ftdna" => "FamilyTreeDNA (array)",
        "livingdna" => "LivingDNA (array)",
        "tellmegen" => "tellmeGen (array)",
        "genesforgood" => "Genes for Good (array)",
        "vcf" => "VCF (variant calls)",
        "gvcf" => "gVCF (whole-genome sequencing)",
        "fastq" => "FASTQ (raw sequencing reads)",
        "bam" => "BAM (aligned reads)",
        "cram" => "CRAM (aligned reads)",
        "sam" => "SAM (aligned reads)",
        _ => "Unknown source",
    }
}

/// Per-genome provenance + quality summary, ready to surface in the UI and store
/// alongside the genome row.
#[derive(Debug, Clone, PartialEq)]
pub struct Provenance {
    /// Human source label, e.g. "23andMe (v5 array)".
    pub source_label: String,
    /// Detected format id, e.g. "23andme_v5".
    pub format: String,
    /// Detected reference build (e.g. "GRCh37"), if any.
    pub build: Option<String>,
    /// Percentage of input lines that were skipped (0.0–100.0, 2 d.p.).
    pub skip_rate: f64,
    /// Plain-language warnings for the user; empty when everything looks clean.
    pub warnings: Vec<String>,
}

/// Round a percentage to two decimal places.
fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Build the provenance summary + warnings for a completed import.
///
/// `total_lines`/`skipped_lines` come from the parse summary; `snp_count` is the
/// number of variants actually stored (used to catch "detected but nothing
/// imported" cases).
pub fn build_provenance(
    format: &str,
    build: Option<&str>,
    total_lines: usize,
    skipped_lines: usize,
    snp_count: usize,
) -> Provenance {
    let skip_rate = if total_lines > 0 {
        round2(skipped_lines as f64 / total_lines as f64 * 100.0)
    } else {
        0.0
    };

    let mut warnings = Vec::new();

    match build {
        None => warnings.push(
            "Reference build could not be detected. Genomic-position lookups \
(e.g. GWAS) assume a build; verify yours before relying on position-based \
results."
                .to_string(),
        ),
        Some(b) if !b.eq_ignore_ascii_case(REFERENCE_BUILD) => warnings.push(format!(
            "This data is reference build {b}, but the bundled annotations use \
{REFERENCE_BUILD}. Positions may not align — use build liftover to harmonize \
{b} → {REFERENCE_BUILD} before position-based analysis."
        )),
        _ => {}
    }

    if snp_count == 0 {
        warnings.push(
            "No variants were imported from this file. It may be truncated, \
empty, or not contain genotype data.".to_string(),
        );
    } else if skip_rate >= 50.0 {
        warnings.push(format!(
            "{skip_rate}% of lines were skipped — unusually high. The file may \
be partially malformed or contain many no-calls."
        ));
    }

    Provenance {
        source_label: source_label(format).to_string(),
        format: format.to_string(),
        build: build.map(str::to_string),
        skip_rate,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_every_supported_format() {
        assert_eq!(source_label("23andme_v5"), "23andMe (v5 array)");
        assert_eq!(source_label("livingdna"), "LivingDNA (array)");
        assert_eq!(source_label("tellmegen"), "tellmeGen (array)");
        assert_eq!(source_label("genesforgood"), "Genes for Good (array)");
        assert_eq!(source_label("gvcf"), "gVCF (whole-genome sequencing)");
        assert_eq!(source_label("bam"), "BAM (aligned reads)");
        assert_eq!(source_label("nope"), "Unknown source");
    }

    #[test]
    fn clean_grch38_import_has_no_warnings() {
        let p = build_provenance("gvcf", Some("GRCh38"), 1000, 100, 900);
        assert_eq!(p.source_label, "gVCF (whole-genome sequencing)");
        assert_eq!(p.skip_rate, 10.0);
        assert!(p.warnings.is_empty(), "unexpected: {:?}", p.warnings);
    }

    #[test]
    fn warns_on_build_mismatch_and_points_to_liftover() {
        let p = build_provenance("23andme_v5", Some("GRCh37"), 1000, 50, 950);
        assert_eq!(p.warnings.len(), 1);
        let w = &p.warnings[0];
        assert!(w.contains("GRCh37"));
        assert!(w.contains("GRCh38"));
        assert!(w.to_lowercase().contains("liftover"));
    }

    #[test]
    fn warns_on_unknown_build() {
        let p = build_provenance("vcf", None, 100, 10, 90);
        assert_eq!(p.warnings.len(), 1);
        assert!(p.warnings[0].contains("build could not be detected"));
    }

    #[test]
    fn warns_on_empty_import_and_high_skip_rate() {
        // Nothing imported.
        let empty = build_provenance("vcf", Some("GRCh38"), 10, 10, 0);
        assert!(empty.warnings.iter().any(|w| w.contains("No variants")));

        // High skip rate (but some variants).
        let noisy = build_provenance("vcf", Some("GRCh38"), 100, 80, 20);
        assert_eq!(noisy.skip_rate, 80.0);
        assert!(noisy.warnings.iter().any(|w| w.contains("skipped")));
    }

    #[test]
    fn zero_lines_does_not_divide_by_zero() {
        let p = build_provenance("vcf", Some("GRCh38"), 0, 0, 0);
        assert_eq!(p.skip_rate, 0.0);
        // snp_count 0 → the "no variants" warning still fires.
        assert!(p.warnings.iter().any(|w| w.contains("No variants")));
    }
}
