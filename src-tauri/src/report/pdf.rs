use std::path::Path;
use std::io::Write;

use rusqlite::Connection;

use crate::analysis::ancestry::analyze_ancestry;
use crate::analysis::carrier::analyze_carrier_status;
use crate::analysis::health_risk::analyze_health_risks;
use crate::analysis::pharmacogenomics::analyze_pharmacogenomics;
use crate::analysis::traits::analyze_traits;
use crate::db::queries;
use crate::error::AppError;

/// Generate a structured text report for the given genome.
///
/// Produces a readable report with sections for all analysis types.
/// The output format is structured text that can be enhanced to PDF in future.
pub fn generate_report(
    conn: &Connection,
    genome_id: i64,
    output_path: &Path,
) -> Result<(), AppError> {
    let genome = queries::get_genome(conn, genome_id)?;
    let chr_counts = queries::get_snp_count_by_chromosome(conn, genome_id)?;

    let mut output = String::new();

    // Title
    output.push_str("═══════════════════════════════════════════════════════════════\n");
    output.push_str("                    GENOME STUDIO REPORT\n");
    output.push_str("                  Personal Genomics Analysis\n");
    output.push_str("═══════════════════════════════════════════════════════════════\n\n");

    output.push_str("DISCLAIMER: This report is for informational and educational\n");
    output.push_str("purposes only. It is NOT a medical diagnosis. Consult a\n");
    output.push_str("healthcare professional or genetic counselor for medical advice.\n\n");

    // Section 1: Overview
    output.push_str("───────────────────────────────────────────────────────────────\n");
    output.push_str("  SECTION 1: GENOME OVERVIEW\n");
    output.push_str("───────────────────────────────────────────────────────────────\n\n");

    output.push_str(&format!("  File:            {}\n", genome.filename));
    output.push_str(&format!("  Format:          {}\n", genome.format));
    output.push_str(&format!("  Import Date:     {}\n", genome.imported_at));
    output.push_str(&format!("  Total SNPs:      {}\n", genome.snp_count));
    output.push_str(&format!(
        "  Reference Build: {}\n",
        genome.build.as_deref().unwrap_or("Unknown")
    ));
    output.push_str(&format!("  Report Generated: {}\n\n", chrono::Utc::now().to_rfc3339()));

    output.push_str("  SNPs by Chromosome:\n");
    for cc in &chr_counts {
        output.push_str(&format!("    Chr {:>2}: {:>6} SNPs\n", cc.chromosome, cc.count));
    }
    output.push('\n');

    // Section 2: Health Risks
    output.push_str("───────────────────────────────────────────────────────────────\n");
    output.push_str("  SECTION 2: HEALTH RISK ANALYSIS\n");
    output.push_str("───────────────────────────────────────────────────────────────\n\n");

    match analyze_health_risks(conn, genome_id) {
        Ok(risks) => {
            if risks.is_empty() {
                output.push_str("  No significant health risk variants detected.\n\n");
            } else {
                for risk in &risks {
                    output.push_str(&format!(
                        "  [{}] {} - {}\n",
                        risk.risk_level.to_uppercase(),
                        risk.condition,
                        risk.category
                    ));
                    output.push_str(&format!(
                        "    Risk Score: {:.0}% | Confidence: {} | Studies: {}\n",
                        risk.score * 100.0,
                        risk.confidence,
                        risk.study_count
                    ));
                    for snp in &risk.contributing_snps {
                        output.push_str(&format!(
                            "    - {} ({}) [{}]: {}\n",
                            snp.rsid, snp.gene, snp.genotype, snp.effect
                        ));
                    }
                    output.push('\n');
                }
            }
        }
        Err(e) => {
            output.push_str(&format!("  Error analyzing health risks: {}\n\n", e));
        }
    }

    // Section 3: Pharmacogenomics
    output.push_str("───────────────────────────────────────────────────────────────\n");
    output.push_str("  SECTION 3: PHARMACOGENOMICS\n");
    output.push_str("───────────────────────────────────────────────────────────────\n\n");

    match analyze_pharmacogenomics(conn, genome_id) {
        Ok(pharma) => {
            if pharma.is_empty() {
                output.push_str("  No pharmacogenomic variants detected.\n\n");
            } else {
                for result in &pharma {
                    output.push_str(&format!(
                        "  {} ({})\n",
                        result.gene, result.star_allele
                    ));
                    output.push_str(&format!(
                        "    Phenotype: {} metabolizer\n",
                        result.phenotype
                    ));
                    output.push_str(&format!(
                        "    Clinical Actionability: {}\n",
                        result.clinical_actionability
                    ));
                    if !result.affected_drugs.is_empty() {
                        output.push_str("    Affected Medications:\n");
                        for drug in &result.affected_drugs {
                            output.push_str(&format!(
                                "    - {} ({}) [Evidence: {}]\n      {}\n",
                                drug.name, drug.category, drug.evidence_level, drug.recommendation
                            ));
                        }
                    }
                    output.push('\n');
                }
            }
        }
        Err(e) => {
            output.push_str(&format!("  Error analyzing pharmacogenomics: {}\n\n", e));
        }
    }

    // Section 4: Traits
    output.push_str("───────────────────────────────────────────────────────────────\n");
    output.push_str("  SECTION 4: TRAIT PREDICTIONS\n");
    output.push_str("───────────────────────────────────────────────────────────────\n\n");

    match analyze_traits(conn, genome_id) {
        Ok(traits) => {
            if traits.is_empty() {
                output.push_str("  No trait-related variants detected.\n\n");
            } else {
                for t in &traits {
                    output.push_str(&format!("  {} ({})\n", t.name, t.category));
                    output.push_str(&format!("    Prediction: {}\n", t.prediction));
                    output.push_str(&format!("    Confidence: {:.0}%\n", t.confidence * 100.0));
                    if let Some(freq) = t.population_frequency {
                        output.push_str(&format!(
                            "    Population Frequency: {:.1}%\n",
                            freq * 100.0
                        ));
                    }
                    output.push_str(&format!("    {}\n", t.description));
                    output.push('\n');
                }
            }
        }
        Err(e) => {
            output.push_str(&format!("  Error analyzing traits: {}\n\n", e));
        }
    }

    // Section 5: Ancestry
    output.push_str("───────────────────────────────────────────────────────────────\n");
    output.push_str("  SECTION 5: ANCESTRY ESTIMATION\n");
    output.push_str("───────────────────────────────────────────────────────────────\n\n");

    match analyze_ancestry(conn, genome_id) {
        Ok(ancestry) => {
            output.push_str("  Population Composition:\n");
            for pop in &ancestry.populations {
                let bar_len = (pop.percentage / 2.0) as usize;
                let bar: String = std::iter::repeat('#').take(bar_len).collect();
                output.push_str(&format!(
                    "    {:>20}: {:>5.1}% {}\n",
                    pop.name, pop.percentage, bar
                ));
            }
            output.push('\n');

            if let Some(ref maternal) = ancestry.maternal_haplogroup {
                output.push_str(&format!("  Maternal Haplogroup (mtDNA): {}\n", maternal));
            }
            if let Some(ref paternal) = ancestry.paternal_haplogroup {
                output.push_str(&format!("  Paternal Haplogroup (Y-DNA): {}\n", paternal));
            }
            output.push('\n');
        }
        Err(e) => {
            output.push_str(&format!("  Error analyzing ancestry: {}\n\n", e));
        }
    }

    // Section 6: Carrier Status
    output.push_str("───────────────────────────────────────────────────────────────\n");
    output.push_str("  SECTION 6: CARRIER STATUS SCREENING\n");
    output.push_str("───────────────────────────────────────────────────────────────\n\n");

    match analyze_carrier_status(conn, genome_id) {
        Ok(carriers) => {
            if carriers.is_empty() {
                output.push_str("  No carrier status results available.\n\n");
            } else {
                for c in &carriers {
                    let status_symbol = match c.status.as_str() {
                        "not_carrier" => "[-]",
                        "carrier" => "[!]",
                        "affected" => "[!!]",
                        _ => "[?]",
                    };
                    output.push_str(&format!(
                        "  {} {} - {} ({})\n",
                        status_symbol, c.condition, c.gene, c.inheritance_pattern
                    ));
                    output.push_str(&format!("    Status: {}\n", c.status));
                    if !c.variants_checked.is_empty() {
                        for v in &c.variants_checked {
                            let carrier_marker = if v.is_carrier { " *CARRIER*" } else { "" };
                            output.push_str(&format!(
                                "    - {} [{}] (pathogenic: {}){}  \n",
                                v.rsid, v.genotype, v.pathogenic_allele, carrier_marker
                            ));
                        }
                    }
                    output.push('\n');
                }
            }
        }
        Err(e) => {
            output.push_str(&format!("  Error analyzing carrier status: {}\n\n", e));
        }
    }

    // Footer
    output.push_str("═══════════════════════════════════════════════════════════════\n");
    output.push_str("  END OF REPORT\n");
    output.push_str("  Generated by Genome Studio - Local DNA Analysis\n");
    output.push_str("  All analysis performed locally on your device.\n");
    output.push_str("  No genetic data was transmitted over the network.\n");
    output.push_str("═══════════════════════════════════════════════════════════════\n");

    // Write to file
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = std::fs::File::create(output_path)?;
    file.write_all(output.as_bytes())?;

    Ok(())
}
