use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// A single population component of the admixture estimate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PopulationComponent {
    pub name: String,
    pub percentage: f64,
    pub color: String,
}

/// Full ancestry analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AncestryResult {
    pub populations: Vec<PopulationComponent>,
    pub maternal_haplogroup: Option<String>,
    pub paternal_haplogroup: Option<String>,
}

/// Ancestry informative marker definition.
struct AncestryMarker {
    rsid: &'static str,
    /// Allele frequencies for each population: (European, African, East Asian, South Asian, Native American)
    freq_eur: f64,
    freq_afr: f64,
    freq_eas: f64,
    freq_sas: f64,
    freq_amr: f64,
    ref_allele: char,
}

const ANCESTRY_MARKERS: &[AncestryMarker] = &[
    AncestryMarker { rsid: "rs3827760", freq_eur: 0.01, freq_afr: 0.01, freq_eas: 0.90, freq_sas: 0.05, freq_amr: 0.70, ref_allele: 'A' },
    AncestryMarker { rsid: "rs1426654", freq_eur: 0.99, freq_afr: 0.05, freq_eas: 0.05, freq_sas: 0.65, freq_amr: 0.40, ref_allele: 'A' },
    AncestryMarker { rsid: "rs16891982", freq_eur: 0.95, freq_afr: 0.02, freq_eas: 0.01, freq_sas: 0.10, freq_amr: 0.30, ref_allele: 'G' },
    AncestryMarker { rsid: "rs2814778", freq_eur: 0.01, freq_afr: 0.99, freq_eas: 0.01, freq_sas: 0.01, freq_amr: 0.15, ref_allele: 'T' },
    AncestryMarker { rsid: "rs1800414", freq_eur: 0.02, freq_afr: 0.01, freq_eas: 0.65, freq_sas: 0.05, freq_amr: 0.10, ref_allele: 'C' },
    AncestryMarker { rsid: "rs1042602", freq_eur: 0.35, freq_afr: 0.03, freq_eas: 0.01, freq_sas: 0.25, freq_amr: 0.15, ref_allele: 'A' },
    AncestryMarker { rsid: "rs12913832", freq_eur: 0.75, freq_afr: 0.02, freq_eas: 0.01, freq_sas: 0.05, freq_amr: 0.10, ref_allele: 'G' },
    AncestryMarker { rsid: "rs885479", freq_eur: 0.15, freq_afr: 0.05, freq_eas: 0.80, freq_sas: 0.30, freq_amr: 0.60, ref_allele: 'T' },
    AncestryMarker { rsid: "rs2031526", freq_eur: 0.10, freq_afr: 0.05, freq_eas: 0.75, freq_sas: 0.15, freq_amr: 0.50, ref_allele: 'G' },
    AncestryMarker { rsid: "rs260690", freq_eur: 0.55, freq_afr: 0.30, freq_eas: 0.70, freq_sas: 0.45, freq_amr: 0.35, ref_allele: 'A' },
];

/// Mitochondrial SNPs for basic maternal haplogroup estimation.
struct MtSnpDef {
    rsid: &'static str,
    position: i64,
    alt_allele: char,
    haplogroup: &'static str,
}

const MT_HAPLOGROUP_SNPS: &[MtSnpDef] = &[
    MtSnpDef { rsid: "rs2853499", position: 7028, alt_allele: 'T', haplogroup: "H" },
    MtSnpDef { rsid: "rs41347846", position: 12308, alt_allele: 'G', haplogroup: "U" },
    MtSnpDef { rsid: "rs28358571", position: 10873, alt_allele: 'C', haplogroup: "L" },
    MtSnpDef { rsid: "rs2857291", position: 8701, alt_allele: 'G', haplogroup: "N" },
    MtSnpDef { rsid: "rs3928305", position: 10400, alt_allele: 'T', haplogroup: "M" },
];

/// Y-chromosome SNPs for basic paternal haplogroup estimation.
struct YSnpDef {
    rsid: &'static str,
    alt_allele: char,
    haplogroup: &'static str,
}

const Y_HAPLOGROUP_SNPS: &[YSnpDef] = &[
    YSnpDef { rsid: "rs9786184", alt_allele: 'A', haplogroup: "R1b" },
    YSnpDef { rsid: "rs9341296", alt_allele: 'C', haplogroup: "R1a" },
    YSnpDef { rsid: "rs2032652", alt_allele: 'A', haplogroup: "E1b" },
    YSnpDef { rsid: "rs13304168", alt_allele: 'C', haplogroup: "I1" },
    YSnpDef { rsid: "rs17250804", alt_allele: 'G', haplogroup: "J2" },
];

/// Analyze ancestry composition using simplified admixture estimation.
pub fn analyze_ancestry(
    conn: &Connection,
    genome_id: i64,
) -> Result<AncestryResult, AppError> {
    // Gather all rsids we need
    let mut all_rsids: Vec<&str> = ANCESTRY_MARKERS.iter().map(|m| m.rsid).collect();
    all_rsids.extend(MT_HAPLOGROUP_SNPS.iter().map(|s| s.rsid));
    all_rsids.extend(Y_HAPLOGROUP_SNPS.iter().map(|s| s.rsid));

    let placeholders: String = all_rsids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT rsid, genotype, chromosome FROM snps WHERE genome_id = ?1 AND rsid IN ({})",
        placeholders
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    params.push(Box::new(genome_id));
    for rsid in &all_rsids {
        params.push(Box::new(rsid.to_string()));
    }
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let user_snps: std::collections::HashMap<String, (String, String)> = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                (row.get::<_, String>(1)?, row.get::<_, String>(2)?),
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Simplified admixture estimation using allele frequencies
    let mut scores = [0.0f64; 5]; // EUR, AFR, EAS, SAS, AMR
    let mut markers_found = 0;

    for marker in ANCESTRY_MARKERS {
        if let Some((genotype, _)) = user_snps.get(marker.rsid) {
            markers_found += 1;
            let ref_count = genotype
                .chars()
                .filter(|c| c.to_ascii_uppercase() == marker.ref_allele.to_ascii_uppercase())
                .count() as f64;
            let allele_dose = ref_count / 2.0; // 0.0, 0.5, or 1.0

            // Bayesian-style likelihood update using allele frequencies
            let freqs = [
                marker.freq_eur,
                marker.freq_afr,
                marker.freq_eas,
                marker.freq_sas,
                marker.freq_amr,
            ];
            for (i, freq) in freqs.iter().enumerate() {
                let likelihood = allele_dose * freq + (1.0 - allele_dose) * (1.0 - freq);
                scores[i] += likelihood.ln();
            }
        }
    }

    // Convert log-likelihoods to percentages
    let populations = if markers_found > 0 {
        // Exponentiate and normalize
        let max_score = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let exp_scores: Vec<f64> = scores.iter().map(|s| (s - max_score).exp()).collect();
        let total: f64 = exp_scores.iter().sum();

        let names = ["European", "African", "East Asian", "South Asian", "Native American"];
        let colors = ["#4A90D9", "#E67E22", "#27AE60", "#9B59B6", "#E74C3C"];

        names
            .iter()
            .zip(colors.iter())
            .zip(exp_scores.iter())
            .map(|((name, color), score)| {
                let pct = (score / total * 1000.0).round() / 10.0;
                PopulationComponent {
                    name: name.to_string(),
                    percentage: pct,
                    color: color.to_string(),
                }
            })
            .filter(|p| p.percentage >= 0.5)
            .collect()
    } else {
        vec![PopulationComponent {
            name: "Insufficient data".to_string(),
            percentage: 100.0,
            color: "#999999".to_string(),
        }]
    };

    // Maternal haplogroup (mtDNA)
    let maternal_haplogroup = MT_HAPLOGROUP_SNPS.iter().find_map(|def| {
        user_snps.get(def.rsid).and_then(|(genotype, chrom)| {
            if chrom == "MT" || chrom == "26" {
                if genotype.contains(def.alt_allele) {
                    Some(def.haplogroup.to_string())
                } else {
                    None
                }
            } else {
                None
            }
        })
    });

    // Paternal haplogroup (Y chromosome)
    let paternal_haplogroup = Y_HAPLOGROUP_SNPS.iter().find_map(|def| {
        user_snps.get(def.rsid).and_then(|(genotype, chrom)| {
            if chrom == "Y" || chrom == "24" {
                if genotype.contains(def.alt_allele) {
                    Some(def.haplogroup.to_string())
                } else {
                    None
                }
            } else {
                None
            }
        })
    });

    Ok(AncestryResult {
        populations,
        maternal_haplogroup,
        paternal_haplogroup,
    })
}
