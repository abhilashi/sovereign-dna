use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// A SNP that contributes to a trait prediction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraitSnp {
    pub rsid: String,
    pub genotype: String,
    pub effect: String,
}

/// Trait analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraitResult {
    pub name: String,
    pub category: String,
    pub prediction: String,
    pub confidence: f64,
    pub description: String,
    pub contributing_snps: Vec<TraitSnp>,
    pub population_frequency: Option<f64>,
    pub source: String,
}

struct TraitSnpDef {
    rsid: &'static str,
    trait_name: &'static str,
    category: &'static str,
    description: &'static str,
    population_frequency: Option<f64>,
    evaluate: fn(&str) -> (&'static str, f64, &'static str),
}

fn eval_eye_color_herc2(gt: &str) -> (&'static str, f64, &'static str) {
    match gt.to_uppercase().as_str() {
        "GG" => ("Likely brown eyes", 0.85, "GG genotype strongly associated with brown eyes"),
        "AG" | "GA" => ("Possibly green or hazel eyes", 0.6, "AG genotype associated with variable eye color"),
        "AA" => ("Likely blue eyes", 0.8, "AA genotype strongly associated with blue eyes"),
        _ => ("Unknown", 0.1, "Genotype not recognized"),
    }
}

fn eval_eye_color_oca2(gt: &str) -> (&'static str, f64, &'static str) {
    match gt.to_uppercase().as_str() {
        "TT" => ("Supports blue eye color", 0.7, "TT associated with blue eyes"),
        "CT" | "TC" => ("Supports intermediate eye color", 0.5, "Heterozygous for eye color variant"),
        "CC" => ("Supports brown eye color", 0.7, "CC associated with brown eyes"),
        _ => ("Unknown", 0.1, "Genotype not recognized"),
    }
}

fn eval_red_hair(gt: &str) -> (&'static str, f64, &'static str) {
    match gt.to_uppercase().as_str() {
        "TT" => ("Likely red hair or carrier", 0.8, "Homozygous MC1R variant, associated with red hair"),
        "CT" | "TC" => ("Possible red hair carrier", 0.5, "Heterozygous MC1R variant"),
        "CC" => ("Unlikely red hair from this variant", 0.7, "Wild-type MC1R"),
        _ => ("Unknown", 0.1, "Genotype not recognized"),
    }
}

fn eval_muscle_type(gt: &str) -> (&'static str, f64, &'static str) {
    match gt.to_uppercase().as_str() {
        "CC" => ("Sprint/power muscle type", 0.75, "CC genotype: functional alpha-actinin-3 protein, favors fast-twitch muscle"),
        "CT" | "TC" => ("Mixed muscle type", 0.6, "CT genotype: intermediate muscle fiber composition"),
        "TT" => ("Endurance muscle type", 0.75, "TT genotype: alpha-actinin-3 deficiency, favors slow-twitch/endurance"),
        _ => ("Unknown", 0.1, "Genotype not recognized"),
    }
}

fn eval_caffeine(gt: &str) -> (&'static str, f64, &'static str) {
    match gt.to_uppercase().as_str() {
        "AA" => ("Fast caffeine metabolizer", 0.8, "AA genotype: rapid caffeine metabolism via CYP1A2"),
        "AC" | "CA" => ("Moderate caffeine metabolizer", 0.7, "AC genotype: intermediate caffeine metabolism"),
        "CC" => ("Slow caffeine metabolizer", 0.8, "CC genotype: slow caffeine metabolism, higher sensitivity"),
        _ => ("Unknown", 0.1, "Genotype not recognized"),
    }
}

fn eval_alcohol_flush(gt: &str) -> (&'static str, f64, &'static str) {
    match gt.to_uppercase().as_str() {
        "GG" => ("Normal alcohol metabolism", 0.85, "GG genotype: functional ALDH2 enzyme"),
        "AG" | "GA" => ("Alcohol flush reaction likely", 0.8, "AG genotype: reduced ALDH2 activity, flush reaction"),
        "AA" => ("Strong alcohol flush reaction", 0.9, "AA genotype: very low ALDH2 activity, severe flush"),
        _ => ("Unknown", 0.1, "Genotype not recognized"),
    }
}

fn eval_bitter_taste_1(gt: &str) -> (&'static str, f64, &'static str) {
    match gt.to_uppercase().as_str() {
        "GG" => ("Likely bitter taster", 0.7, "PAV haplotype component (taster)"),
        "CG" | "GC" => ("Intermediate bitter taste", 0.5, "Heterozygous taster variant"),
        "CC" => ("Likely non-taster", 0.7, "AVI haplotype component (non-taster)"),
        _ => ("Unknown", 0.1, "Genotype not recognized"),
    }
}

fn eval_bitter_taste_2(gt: &str) -> (&'static str, f64, &'static str) {
    match gt.to_uppercase().as_str() {
        "CC" => ("Supports taster phenotype", 0.6, "TAS2R38 variant supporting taste sensitivity"),
        "CT" | "TC" => ("Intermediate", 0.4, "Heterozygous"),
        "TT" => ("Supports non-taster phenotype", 0.6, "TAS2R38 non-taster variant"),
        _ => ("Unknown", 0.1, "Genotype not recognized"),
    }
}

fn eval_bitter_taste_3(gt: &str) -> (&'static str, f64, &'static str) {
    match gt.to_uppercase().as_str() {
        "CC" => ("Supports taster phenotype", 0.6, "TAS2R38 variant supporting taste sensitivity"),
        "CG" | "GC" => ("Intermediate", 0.4, "Heterozygous"),
        "GG" => ("Supports non-taster phenotype", 0.6, "TAS2R38 non-taster variant"),
        _ => ("Unknown", 0.1, "Genotype not recognized"),
    }
}

fn eval_cilantro(gt: &str) -> (&'static str, f64, &'static str) {
    match gt.to_uppercase().as_str() {
        "CC" => ("Likely enjoys cilantro", 0.7, "CC genotype: typical cilantro perception"),
        "CA" | "AC" => ("May perceive soapy taste", 0.6, "CA genotype: partial soapy taste sensitivity"),
        "AA" => ("Likely perceives cilantro as soapy", 0.8, "AA genotype: OR6A2 variant associated with soapy taste"),
        _ => ("Unknown", 0.1, "Genotype not recognized"),
    }
}

const TRAIT_DEFS: &[TraitSnpDef] = &[
    TraitSnpDef { rsid: "rs12913832", trait_name: "Eye Color", category: "Appearance", description: "HERC2 gene variant is the primary determinant of blue vs. brown eye color", population_frequency: Some(0.25), evaluate: eval_eye_color_herc2 },
    TraitSnpDef { rsid: "rs1800407", trait_name: "Eye Color (modifier)", category: "Appearance", description: "OCA2 variant modifies eye color determination", population_frequency: Some(0.08), evaluate: eval_eye_color_oca2 },
    TraitSnpDef { rsid: "rs1805007", trait_name: "Red Hair", category: "Appearance", description: "MC1R gene variant strongly associated with red hair and fair skin", population_frequency: Some(0.10), evaluate: eval_red_hair },
    TraitSnpDef { rsid: "rs1815739", trait_name: "Muscle Fiber Type", category: "Athletic Performance", description: "ACTN3 gene determines alpha-actinin-3 presence in fast-twitch muscle fibers", population_frequency: Some(0.18), evaluate: eval_muscle_type },
    TraitSnpDef { rsid: "rs762551", trait_name: "Caffeine Metabolism", category: "Nutrition", description: "CYP1A2 enzyme activity determines how quickly you metabolize caffeine", population_frequency: Some(0.46), evaluate: eval_caffeine },
    TraitSnpDef { rsid: "rs671", trait_name: "Alcohol Flush Reaction", category: "Nutrition", description: "ALDH2 variant causes acetaldehyde accumulation and facial flushing after alcohol", population_frequency: Some(0.08), evaluate: eval_alcohol_flush },
    TraitSnpDef { rsid: "rs713598", trait_name: "Bitter Taste Perception", category: "Nutrition", description: "TAS2R38 gene determines sensitivity to bitter compounds like PTC and PROP", population_frequency: Some(0.50), evaluate: eval_bitter_taste_1 },
    TraitSnpDef { rsid: "rs1726866", trait_name: "Bitter Taste Perception (modifier)", category: "Nutrition", description: "Additional TAS2R38 variant contributing to bitter taste sensitivity", population_frequency: Some(0.45), evaluate: eval_bitter_taste_2 },
    TraitSnpDef { rsid: "rs10246939", trait_name: "Bitter Taste Perception (modifier 2)", category: "Nutrition", description: "Third TAS2R38 variant for complete bitter taste haplotype", population_frequency: Some(0.45), evaluate: eval_bitter_taste_3 },
    TraitSnpDef { rsid: "rs72921001", trait_name: "Cilantro/Coriander Aversion", category: "Nutrition", description: "OR6A2 olfactory receptor variant associated with perceiving cilantro as soapy", population_frequency: Some(0.15), evaluate: eval_cilantro },
];

/// Analyze trait predictions based on the user's genotype data.
pub fn analyze_traits(
    conn: &Connection,
    genome_id: i64,
) -> Result<Vec<TraitResult>, AppError> {
    let rsids: Vec<&str> = TRAIT_DEFS.iter().map(|d| d.rsid).collect();

    let placeholders: String = rsids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT rsid, genotype FROM snps WHERE genome_id = ?1 AND rsid IN ({})",
        placeholders
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    params.push(Box::new(genome_id));
    for rsid in &rsids {
        params.push(Box::new(rsid.to_string()));
    }
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let user_snps: std::collections::HashMap<String, String> = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let mut results = Vec::new();

    for def in TRAIT_DEFS {
        if let Some(genotype) = user_snps.get(def.rsid) {
            let (prediction, confidence, effect) = (def.evaluate)(genotype);

            results.push(TraitResult {
                name: def.trait_name.to_string(),
                category: def.category.to_string(),
                prediction: prediction.to_string(),
                confidence,
                description: def.description.to_string(),
                contributing_snps: vec![TraitSnp {
                    rsid: def.rsid.to_string(),
                    genotype: genotype.clone(),
                    effect: effect.to_string(),
                }],
                population_frequency: def.population_frequency,
                source: "curated".to_string(),
            });
        }
    }

    // Collect trait names already covered for deduplication
    let curated_traits: std::collections::HashSet<String> = results
        .iter()
        .map(|r| r.name.to_lowercase())
        .collect();

    // Disease-like terms to exclude from GWAS trait results
    let disease_terms = [
        "disease", "cancer", "disorder", "syndrome", "carcinoma", "diabetes",
        "schizophrenia", "asthma", "arthritis", "lupus", "sclerosis",
        "fibrosis", "anemia", "leukemia", "lymphoma", "melanoma",
    ];

    // --- GWAS Catalog enrichment for traits ---
    if is_reference_ready(conn, "gwas_catalog") {
        let mut gwas_stmt = conn.prepare(
            "SELECT g.rsid, g.trait_name, g.odds_ratio, g.p_value, s.genotype
             FROM gwas_associations g
             INNER JOIN snps s ON g.rsid = s.rsid
             WHERE s.genome_id = ?1 AND g.p_value < 5e-8"
        )?;

        let gwas_rows: Vec<(String, String, Option<f64>, f64, String)> = gwas_stmt
            .query_map([genome_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        // Group by trait_name, filtering out disease-like traits
        let mut trait_groups: std::collections::HashMap<
            String,
            Vec<(String, Option<f64>, f64, String)>,
        > = std::collections::HashMap::new();
        for (rsid, trait_name, odds_ratio, p_value, genotype) in gwas_rows {
            let trait_lower = trait_name.to_lowercase();
            // Skip disease-like traits
            if disease_terms.iter().any(|term| trait_lower.contains(term)) {
                continue;
            }
            // Skip already covered traits
            if curated_traits.contains(&trait_lower) {
                continue;
            }
            trait_groups
                .entry(trait_name)
                .or_default()
                .push((rsid, odds_ratio, p_value, genotype));
        }

        for (trait_name, associations) in trait_groups {
            let contributing_snps: Vec<TraitSnp> = associations
                .iter()
                .map(|(rsid, odds_ratio, p_value, genotype)| TraitSnp {
                    rsid: rsid.clone(),
                    genotype: genotype.clone(),
                    effect: format!(
                        "p={:.2e}{}",
                        p_value,
                        odds_ratio.map(|or| format!(", OR={:.2}", or)).unwrap_or_default()
                    ),
                })
                .collect();

            // Best p-value gives higher confidence
            let best_p = associations
                .iter()
                .map(|(_, _, p, _)| *p)
                .fold(f64::MAX, f64::min);
            let confidence = if best_p < 1e-20 {
                0.8
            } else if best_p < 1e-12 {
                0.6
            } else {
                0.4
            };

            let prediction = associations
                .first()
                .map(|(_, _, _, gt)| gt.clone())
                .unwrap_or_default();

            results.push(TraitResult {
                name: trait_name.clone(),
                category: "GWAS".to_string(),
                prediction: format!("Genotype: {}", prediction),
                confidence,
                description: format!("GWAS-associated trait with {} significant variant(s)", contributing_snps.len()),
                contributing_snps,
                population_frequency: None,
                source: "gwas_catalog".to_string(),
            });
        }
    }

    // --- SNPedia enrichment ---
    if is_reference_ready(conn, "snpedia") {
        let mut snpedia_stmt = conn.prepare(
            "SELECT se.rsid, se.genotype, se.magnitude, se.summary, s.genotype AS user_genotype
             FROM snpedia_entries se
             INNER JOIN snps s ON se.rsid = s.rsid
             WHERE s.genome_id = ?1
               AND se.magnitude > 1.0
               AND se.summary IS NOT NULL
               AND se.summary != ''"
        )?;

        let snpedia_rows: Vec<(String, String, f64, String, String)> = snpedia_stmt
            .query_map([genome_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        for (rsid, entry_genotype, magnitude, summary, user_genotype) in snpedia_rows {
            // Only include if the user's genotype matches this snpedia entry's genotype
            if !genotypes_match(&user_genotype, &entry_genotype) {
                continue;
            }

            let summary_lower = summary.to_lowercase();

            // Skip disease-like entries
            if disease_terms.iter().any(|term| summary_lower.contains(term)) {
                continue;
            }

            // Skip if a trait with same name already exists
            let trait_name = format!("{} ({})", rsid, entry_genotype);
            if curated_traits.contains(&trait_name.to_lowercase()) {
                continue;
            }

            let confidence = if magnitude > 3.0 {
                0.8
            } else if magnitude > 2.0 {
                0.6
            } else {
                0.4
            };

            results.push(TraitResult {
                name: trait_name,
                category: "SNPedia".to_string(),
                prediction: user_genotype.clone(),
                confidence,
                description: summary,
                contributing_snps: vec![TraitSnp {
                    rsid: rsid.clone(),
                    genotype: user_genotype.clone(),
                    effect: format!("Magnitude: {:.1}", magnitude),
                }],
                population_frequency: None,
                source: "snpedia".to_string(),
            });
        }
    }

    Ok(results)
}

/// Check if two genotype strings match (order-insensitive, e.g., "AG" matches "GA").
fn genotypes_match(user: &str, entry: &str) -> bool {
    let u = user.to_uppercase();
    let e = entry.to_uppercase();
    if u == e {
        return true;
    }
    // Try reversed
    let reversed: String = e.chars().rev().collect();
    u == reversed
}

/// Check if a reference database is downloaded and ready.
fn is_reference_ready(conn: &Connection, source: &str) -> bool {
    conn.query_row(
        "SELECT status FROM reference_status WHERE source = ?1",
        [source],
        |row| row.get::<_, String>(0),
    )
    .map(|status| status == "ready")
    .unwrap_or(false)
}
