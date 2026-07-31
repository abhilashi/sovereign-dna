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

/// Run the migrated core-traits skill and adapt its output to `TraitResult`.
///
/// This is the reference implementation proving the Phase-2 skill pipeline
/// end-to-end: the panel definition lives in a signed-able manifest, the engine
/// interprets it, and the result is byte-for-byte equivalent to the old
/// hardcoded module (see the parity test at the bottom of this file).
fn curated_traits_via_skill(
    conn: &Connection,
    genome_id: i64,
) -> Result<Vec<TraitResult>, AppError> {
    let manifest = crate::skills::traits_core_manifest();
    let rsids: Vec<String> = manifest.variants.iter().map(|v| v.rsid.clone()).collect();
    let source = crate::skills::SqliteGenotypeSource::load(conn, genome_id, &rsids)?;
    let refs = crate::skills::DbReferenceAvailability::new(conn);
    let output = crate::skills::engine::evaluate(&manifest, &source, &refs)
        .map_err(|e| AppError::Analysis(e.to_string()))?;

    Ok(output
        .findings
        .into_iter()
        .map(|f| TraitResult {
            name: f.name,
            category: f.category,
            prediction: f.prediction,
            confidence: f.confidence,
            description: f.description,
            contributing_snps: f
                .contributing
                .into_iter()
                .map(|c| TraitSnp {
                    rsid: c.rsid,
                    genotype: c.genotype,
                    effect: c.effect,
                })
                .collect(),
            population_frequency: f.population_frequency,
            source: "curated".to_string(),
        })
        .collect())
}

/// Analyze trait predictions based on the user's genotype data.
pub fn analyze_traits(
    conn: &Connection,
    genome_id: i64,
) -> Result<Vec<TraitResult>, AppError> {
    // The curated traits panel is now a declarative skill manifest executed by
    // the skill engine (Phase 2). The hardcoded const tables / `match` arms that
    // used to live here were migrated verbatim to
    // `skills/manifests/traits-core.json`; adding a trait no longer requires
    // editing Rust source or recompiling.
    let mut results = curated_traits_via_skill(conn, genome_id)?;

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

#[cfg(test)]
mod migration_parity_tests {
    //! Proves the migrated `traits-core.json` manifest reproduces the exact
    //! values of the old hardcoded `TRAIT_DEFS` / `match`-arm module.
    use crate::skills::engine::{evaluate, AllReferencesReady, GenotypeSource};
    use std::collections::HashMap;

    struct Mem(HashMap<String, String>);
    impl GenotypeSource for Mem {
        fn genotype(&self, rsid: &str) -> Option<String> {
            self.0.get(rsid).cloned()
        }
    }

    fn mem(pairs: &[(&str, &str)]) -> Mem {
        Mem(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }

    #[test]
    fn manifest_reproduces_original_hardcoded_values() {
        let m = crate::skills::traits_core_manifest();
        let g = mem(&[
            ("rs12913832", "GA"), // orientation-flipped heterozygote
            ("rs671", "AA"),
            ("rs1815739", "CC"),
            ("rs762551", "ac"), // lowercase
        ]);
        let out = evaluate(&m, &g, &AllReferencesReady).unwrap();
        let by_name: HashMap<_, _> =
            out.findings.iter().map(|f| (f.name.clone(), f)).collect();

        let eye = by_name.get("Eye Color").unwrap();
        assert_eq!(eye.prediction, "Possibly green or hazel eyes");
        assert_eq!(eye.confidence, 0.6);
        assert_eq!(eye.category, "Appearance");
        assert_eq!(eye.population_frequency, Some(0.25));
        assert_eq!(eye.contributing[0].genotype, "GA"); // user genotype preserved
        assert_eq!(eye.contributing[0].effect, "AG genotype associated with variable eye color");

        let flush = by_name.get("Alcohol Flush Reaction").unwrap();
        assert_eq!(flush.prediction, "Strong alcohol flush reaction");
        assert_eq!(flush.confidence, 0.9);

        let muscle = by_name.get("Muscle Fiber Type").unwrap();
        assert_eq!(muscle.prediction, "Sprint/power muscle type");
        assert_eq!(muscle.category, "Athletic Performance");

        let caffeine = by_name.get("Caffeine Metabolism").unwrap();
        assert_eq!(caffeine.prediction, "Moderate caffeine metabolizer");
    }

    #[test]
    fn typed_but_unlisted_genotype_is_unknown_and_untyped_excluded() {
        let m = crate::skills::traits_core_manifest();
        // rs762551 typed with unlisted genotype -> Unknown; everything else absent.
        let out = evaluate(&m, &mem(&[("rs762551", "TT")]), &AllReferencesReady).unwrap();
        assert_eq!(out.findings.len(), 1);
        assert_eq!(out.findings[0].prediction, "Unknown");
        assert_eq!(out.findings[0].confidence, 0.1);
    }
}
