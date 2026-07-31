//! Declarative skill engine (Phase 2.2).
//!
//! Executes a [`SkillManifest`] against a genome without recompiling the app and
//! without executing arbitrary native code — the "thousands of simple
//! variant-panel skills" case is pure data interpretation, which is both cheap
//! and safe (a manifest cannot read the filesystem or the network; it can only
//! declare rsIDs to look up and outcomes to report).
//!
//! The engine is deliberately decoupled from the database via the
//! [`GenotypeSource`] trait so it can be unit-tested with an in-memory genome and
//! reused by any storage backend.

use serde::{Deserialize, Serialize};

use super::manifest::{ReferenceDep, SkillManifest, SkillMethod};

/// Abstraction over "the genome we're analysing".
///
/// The real app implements this over the local SQLite `snps` table; tests use an
/// in-memory map. Keeping the engine off `rusqlite` makes it trivially testable
/// and keeps genome access at one auditable choke point.
pub trait GenotypeSource {
    /// Return the genotype string (e.g. `"AG"`) for `rsid` in this genome, if the
    /// variant was typed. `None` means "not present in this genome".
    fn genotype(&self, rsid: &str) -> Option<String>;
}

/// Whether a declared reference dependency is available + ready.
pub trait ReferenceAvailability {
    fn is_ready(&self, dep: ReferenceDep) -> bool;
}

/// A source that reports every reference DB as available (used when a skill
/// declares no deps, and in tests).
pub struct AllReferencesReady;
impl ReferenceAvailability for AllReferencesReady {
    fn is_ready(&self, _dep: ReferenceDep) -> bool {
        true
    }
}

/// One variant that contributed to a finding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributingVariant {
    pub rsid: String,
    pub genotype: String,
    pub effect: String,
}

/// A single result produced by a skill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub name: String,
    pub category: String,
    pub prediction: String,
    pub confidence: f64,
    pub description: String,
    pub contributing: Vec<ContributingVariant>,
    pub population_frequency: Option<f64>,
}

/// The full output of running one skill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillOutput {
    pub skill_id: String,
    pub skill_version: String,
    pub evidence_tier: super::manifest::EvidenceTier,
    pub disclaimer: String,
    pub citations: Vec<String>,
    pub findings: Vec<Finding>,
}

/// Reasons a skill cannot be executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    Invalid(String),
    MissingReference(ReferenceDep),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::Invalid(m) => write!(f, "invalid skill: {m}"),
            EngineError::MissingReference(d) => {
                write!(f, "required reference '{}' is not ready", d.source_key())
            }
        }
    }
}

impl std::error::Error for EngineError {}

/// Normalize a genotype for orientation/case-insensitive comparison.
///
/// Uppercases and sorts the alleles so that `"AG"`, `"ag"` and `"GA"` all map to
/// the same key. No-calls (`"--"`, `".."`, `""`) normalise to `"--"`. This matches
/// (and generalises) the historical `match "AG" | "GA"` arms.
pub fn normalize_genotype(gt: &str) -> String {
    let trimmed = gt.trim();
    if trimmed.is_empty() || trimmed.chars().all(|c| c == '-' || c == '.') {
        return "--".to_string();
    }
    let mut chars: Vec<char> = trimmed.to_uppercase().chars().collect();
    chars.sort_unstable();
    chars.into_iter().collect()
}

/// The default outcome for a typed-but-unrecognised genotype, matching the
/// historical `_ => ("Unknown", 0.1, "Genotype not recognized")` fall-through.
const UNKNOWN_PREDICTION: &str = "Unknown";
const UNKNOWN_CONFIDENCE: f64 = 0.1;
const UNKNOWN_EFFECT: &str = "Genotype not recognized";

/// Execute a validated skill against a genome.
///
/// Semantics preserved from the hardcoded modules:
/// * a variant absent from the genome yields **no** finding;
/// * a variant present but with an unlisted genotype yields an `Unknown` finding
///   (confidence 0.1) for `GenotypeMap`;
/// * declared reference deps must be ready or the whole skill errors out (no
///   silent computation against missing data).
pub fn evaluate<G: GenotypeSource, R: ReferenceAvailability>(
    manifest: &SkillManifest,
    genome: &G,
    refs: &R,
) -> Result<SkillOutput, EngineError> {
    manifest
        .validate()
        .map_err(|e| EngineError::Invalid(e.0))?;

    for dep in &manifest.reference_deps {
        if !refs.is_ready(*dep) {
            return Err(EngineError::MissingReference(*dep));
        }
    }

    let findings = match manifest.method {
        SkillMethod::GenotypeMap => eval_genotype_map(manifest, genome),
        SkillMethod::WeightedSum => eval_weighted_sum(manifest, genome),
    };

    Ok(SkillOutput {
        skill_id: manifest.id.clone(),
        skill_version: manifest.version.clone(),
        evidence_tier: manifest.evidence_tier,
        disclaimer: manifest.disclaimer.clone(),
        citations: manifest.citations.clone(),
        findings,
    })
}

fn eval_genotype_map<G: GenotypeSource>(manifest: &SkillManifest, genome: &G) -> Vec<Finding> {
    let mut findings = Vec::new();
    for rule in &manifest.variants {
        let Some(user_gt) = genome.genotype(&rule.rsid) else {
            continue; // not typed -> no finding
        };
        let norm = normalize_genotype(&user_gt);
        let matched = rule
            .genotypes
            .iter()
            .find(|o| normalize_genotype(&o.genotype) == norm);

        let (prediction, confidence, effect) = match matched {
            Some(o) => (o.prediction.clone(), o.confidence, o.effect.clone()),
            None => (
                UNKNOWN_PREDICTION.to_string(),
                UNKNOWN_CONFIDENCE,
                UNKNOWN_EFFECT.to_string(),
            ),
        };

        findings.push(Finding {
            name: rule.label.clone().unwrap_or_else(|| manifest.name.clone()),
            category: rule
                .category
                .clone()
                .unwrap_or_else(|| manifest.category.clone()),
            prediction,
            confidence,
            description: rule.description.clone().unwrap_or_default(),
            contributing: vec![ContributingVariant {
                rsid: rule.rsid.clone(),
                genotype: user_gt,
                effect,
            }],
            population_frequency: rule.population_frequency,
        });
    }
    findings
}

fn eval_weighted_sum<G: GenotypeSource>(manifest: &SkillManifest, genome: &G) -> Vec<Finding> {
    let mut score = 0.0f64;
    let mut contributing = Vec::new();
    let mut matched_any = false;

    for rule in &manifest.variants {
        let Some(user_gt) = genome.genotype(&rule.rsid) else {
            continue;
        };
        let norm = normalize_genotype(&user_gt);
        if let Some(o) = rule
            .genotypes
            .iter()
            .find(|o| normalize_genotype(&o.genotype) == norm)
        {
            let w = o.weight.unwrap_or(0.0);
            score += w;
            matched_any = true;
            contributing.push(ContributingVariant {
                rsid: rule.rsid.clone(),
                genotype: user_gt,
                effect: format!("{} (weight {:+.2})", o.effect, w),
            });
        }
    }

    if !matched_any {
        return Vec::new();
    }

    vec![Finding {
        name: manifest.name.clone(),
        category: manifest.category.clone(),
        prediction: format!("Aggregate score: {score:.2}"),
        // Confidence scales with how many variants were observed, capped at 0.9.
        confidence: (0.3 + 0.1 * contributing.len() as f64).min(0.9),
        description: manifest.description.clone(),
        contributing,
        population_frequency: None,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::manifest::{EvidenceTier, GenotypeOutcome, VariantRule};
    use std::collections::HashMap;

    struct MapGenome(HashMap<String, String>);
    impl GenotypeSource for MapGenome {
        fn genotype(&self, rsid: &str) -> Option<String> {
            self.0.get(rsid).cloned()
        }
    }

    fn genome(pairs: &[(&str, &str)]) -> MapGenome {
        MapGenome(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }

    fn gmap_manifest() -> SkillManifest {
        SkillManifest {
            schema_version: 1,
            id: "t.gmap".into(),
            version: "1.0.0".into(),
            name: "Skill".into(),
            description: "d".into(),
            category: "Cat".into(),
            method: SkillMethod::GenotypeMap,
            reference_deps: vec![],
            variants: vec![VariantRule {
                rsid: "rs1".into(),
                label: Some("Eye Color".into()),
                category: Some("Appearance".into()),
                description: Some("desc".into()),
                population_frequency: Some(0.25),
                genotypes: vec![
                    GenotypeOutcome { genotype: "GG".into(), prediction: "brown".into(), confidence: 0.85, effect: "GG".into(), weight: None },
                    GenotypeOutcome { genotype: "AG".into(), prediction: "hazel".into(), confidence: 0.6, effect: "AG".into(), weight: None },
                    GenotypeOutcome { genotype: "AA".into(), prediction: "blue".into(), confidence: 0.8, effect: "AA".into(), weight: None },
                ],
            }],
            citations: vec!["PMID:123".into()],
            evidence_tier: EvidenceTier::Verified,
            disclaimer: "not medical advice".into(),
        }
    }

    #[test]
    fn normalize_is_orientation_and_case_insensitive() {
        assert_eq!(normalize_genotype("GA"), "AG");
        assert_eq!(normalize_genotype("ag"), "AG");
        assert_eq!(normalize_genotype("AG"), "AG");
        assert_eq!(normalize_genotype("--"), "--");
        assert_eq!(normalize_genotype(""), "--");
        assert_eq!(normalize_genotype(".."), "--");
    }

    #[test]
    fn genotype_map_matches_orientation_flipped() {
        let out = evaluate(&gmap_manifest(), &genome(&[("rs1", "GA")]), &AllReferencesReady).unwrap();
        assert_eq!(out.findings.len(), 1);
        let f = &out.findings[0];
        assert_eq!(f.name, "Eye Color");
        assert_eq!(f.prediction, "hazel");
        assert_eq!(f.confidence, 0.6);
        assert_eq!(f.contributing[0].genotype, "GA"); // original preserved
        assert_eq!(f.population_frequency, Some(0.25));
    }

    #[test]
    fn absent_variant_yields_no_finding() {
        let out = evaluate(&gmap_manifest(), &genome(&[("rsOther", "AA")]), &AllReferencesReady).unwrap();
        assert!(out.findings.is_empty());
    }

    #[test]
    fn present_but_unlisted_genotype_is_unknown() {
        let out = evaluate(&gmap_manifest(), &genome(&[("rs1", "CT")]), &AllReferencesReady).unwrap();
        assert_eq!(out.findings.len(), 1);
        assert_eq!(out.findings[0].prediction, "Unknown");
        assert_eq!(out.findings[0].confidence, 0.1);
    }

    #[test]
    fn output_carries_provenance() {
        let out = evaluate(&gmap_manifest(), &genome(&[("rs1", "GG")]), &AllReferencesReady).unwrap();
        assert_eq!(out.skill_id, "t.gmap");
        assert_eq!(out.evidence_tier, EvidenceTier::Verified);
        assert_eq!(out.citations, vec!["PMID:123".to_string()]);
        assert!(!out.disclaimer.is_empty());
    }

    #[test]
    fn missing_reference_dep_errors() {
        struct NoRefs;
        impl ReferenceAvailability for NoRefs {
            fn is_ready(&self, _d: ReferenceDep) -> bool {
                false
            }
        }
        let mut m = gmap_manifest();
        m.reference_deps = vec![ReferenceDep::GwasCatalog];
        let err = evaluate(&m, &genome(&[("rs1", "GG")]), &NoRefs).unwrap_err();
        assert_eq!(err, EngineError::MissingReference(ReferenceDep::GwasCatalog));
    }

    #[test]
    fn weighted_sum_aggregates() {
        let mut m = gmap_manifest();
        m.method = SkillMethod::WeightedSum;
        for v in &mut m.variants {
            for g in &mut v.genotypes {
                g.weight = Some(1.5);
            }
        }
        let out = evaluate(&m, &genome(&[("rs1", "GG")]), &AllReferencesReady).unwrap();
        assert_eq!(out.findings.len(), 1);
        assert_eq!(out.findings[0].prediction, "Aggregate score: 1.50");
    }
}
