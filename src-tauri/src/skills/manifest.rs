//! Skill manifest schema (Phase 2.1).
//!
//! A *skill* is a declarative, versioned analysis module. Historically the app's
//! analyses were hardcoded Rust `const` tables and `match` arms (see
//! `analysis/traits.rs`), which meant adding an analysis required editing source
//! and recompiling the whole app. A `SkillManifest` externalises that same
//! information into data so the library can scale to hundreds/thousands of
//! analyses without recompiling — and, in later sub-phases, so skills can be
//! signed, content-addressed, distributed, and sandboxed.
//!
//! This module defines only the *schema*. Execution lives in [`crate::skills::engine`].

use serde::{Deserialize, Serialize};

/// The manifest schema version this build understands.
///
/// Bumped when the on-disk manifest format changes in a backwards-incompatible
/// way. A manifest declaring a higher version than this is rejected by
/// [`SkillManifest::validate`].
pub const SCHEMA_VERSION: u32 = 1;

/// Evidence quality of a skill's results.
///
/// Guards against presenting thousands of user-contributed skills as equal
/// medical truth (spec §2.9 / §6.4). Surfaced on every result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceTier {
    /// Community-contributed, not independently reviewed.
    Community,
    /// Reviewed by curators for basic scientific validity.
    Verified,
    /// Clinical-grade (e.g. ClinVar review status / CPIC-backed).
    Clinical,
}

/// A reference database a skill may depend on.
///
/// The engine (and, later, the registry) refuses to run a skill whose declared
/// dependencies are not present + ready, so results are never silently computed
/// against missing data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceDep {
    Clinvar,
    GwasCatalog,
    Snpedia,
    Pharmgkb,
}

impl ReferenceDep {
    /// The `reference_status.source` key this dependency maps to.
    pub fn source_key(&self) -> &'static str {
        match self {
            ReferenceDep::Clinvar => "clinvar",
            ReferenceDep::GwasCatalog => "gwas_catalog",
            ReferenceDep::Snpedia => "snpedia",
            ReferenceDep::Pharmgkb => "pharmgkb",
        }
    }
}

/// How a skill combines its variant rules into results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillMethod {
    /// Each variant rule is independent: every matched rule yields one finding.
    /// This is exactly the historical `traits.rs` / per-genotype `match` pattern.
    GenotypeMap,
    /// Sum the numeric `weight` of every matched genotype into one aggregate
    /// score finding (the health-risk / simple-PRS pattern).
    WeightedSum,
}

/// The outcome for one specific genotype at a variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenotypeOutcome {
    /// The genotype this outcome applies to, e.g. `"GG"` or `"AG"`.
    ///
    /// Matched **case-insensitively and orientation-insensitively**: `"AG"`,
    /// `"ag"` and `"GA"` are all the same genotype (see
    /// [`crate::skills::engine::normalize_genotype`]). List each genotype once.
    pub genotype: String,
    /// Predicted phenotype / label shown to the user.
    pub prediction: String,
    /// Confidence in `[0.0, 1.0]`.
    pub confidence: f64,
    /// Human-readable effect / explanation.
    pub effect: String,
    /// Numeric contribution for [`SkillMethod::WeightedSum`]; ignored by
    /// `GenotypeMap`.
    #[serde(default)]
    pub weight: Option<f64>,
}

/// One variant the skill reads plus its per-genotype interpretation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantRule {
    /// dbSNP rsID selector.
    ///
    /// Phase-2 scope is rsID panels (matching the current data model); richer
    /// `(chr,pos,ref,alt)` selectors arrive when the Phase-1.3 keying lands.
    pub rsid: String,
    /// Per-finding name for `GenotypeMap` (e.g. `"Eye Color"`). Falls back to the
    /// skill name when absent.
    #[serde(default)]
    pub label: Option<String>,
    /// Per-finding category override for `GenotypeMap`.
    #[serde(default)]
    pub category: Option<String>,
    /// Per-finding description.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional population frequency of the effect genotype.
    #[serde(default)]
    pub population_frequency: Option<f64>,
    /// Outcomes keyed by genotype.
    pub genotypes: Vec<GenotypeOutcome>,
}

/// A declarative, versioned analysis skill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillManifest {
    /// Manifest schema version. Must be `<= SCHEMA_VERSION`.
    pub schema_version: u32,
    /// Stable, unique skill id (reverse-DNS style), e.g.
    /// `"org.sovereigndna.traits.core"`.
    pub id: String,
    /// Semantic version of the skill *content* (independent of schema version).
    pub version: String,
    /// Human-readable name.
    pub name: String,
    /// One-line description.
    pub description: String,
    /// Default category for findings.
    pub category: String,
    /// How variant rules combine.
    pub method: SkillMethod,
    /// Reference databases that must be ready before this skill runs.
    #[serde(default)]
    pub reference_deps: Vec<ReferenceDep>,
    /// The variant rules.
    pub variants: Vec<VariantRule>,
    /// Literature / source citations.
    #[serde(default)]
    pub citations: Vec<String>,
    /// Evidence tier surfaced on every result.
    pub evidence_tier: EvidenceTier,
    /// Mandatory user-facing disclaimer.
    pub disclaimer: String,
}

/// A structured validation error for a manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestError(pub String);

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid skill manifest: {}", self.0)
    }
}

impl std::error::Error for ManifestError {}

impl SkillManifest {
    /// Parse a manifest from JSON, validating it in the process.
    pub fn from_json(s: &str) -> Result<Self, ManifestError> {
        let manifest: SkillManifest =
            serde_json::from_str(s).map_err(|e| ManifestError(format!("json: {e}")))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Structurally validate the manifest. Returns the first problem found.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version == 0 || self.schema_version > SCHEMA_VERSION {
            return Err(ManifestError(format!(
                "unsupported schemaVersion {} (this build supports 1..={})",
                self.schema_version, SCHEMA_VERSION
            )));
        }
        if self.id.trim().is_empty() {
            return Err(ManifestError("id must not be empty".into()));
        }
        if self.version.trim().is_empty() {
            return Err(ManifestError("version must not be empty".into()));
        }
        if self.name.trim().is_empty() {
            return Err(ManifestError("name must not be empty".into()));
        }
        if self.disclaimer.trim().is_empty() {
            return Err(ManifestError("disclaimer is mandatory".into()));
        }
        if self.variants.is_empty() {
            return Err(ManifestError("a skill must declare at least one variant".into()));
        }
        for (i, v) in self.variants.iter().enumerate() {
            if v.rsid.trim().is_empty() {
                return Err(ManifestError(format!("variants[{i}].rsid is empty")));
            }
            if v.genotypes.is_empty() {
                return Err(ManifestError(format!(
                    "variants[{i}] ({}) has no genotype outcomes",
                    v.rsid
                )));
            }
            for (j, g) in v.genotypes.iter().enumerate() {
                if !(0.0..=1.0).contains(&g.confidence) {
                    return Err(ManifestError(format!(
                        "variants[{i}].genotypes[{j}].confidence {} out of [0,1]",
                        g.confidence
                    )));
                }
                if g.genotype.trim().is_empty() {
                    return Err(ManifestError(format!(
                        "variants[{i}].genotypes[{j}].genotype is empty"
                    )));
                }
                if self.method == SkillMethod::WeightedSum && g.weight.is_none() {
                    return Err(ManifestError(format!(
                        "variants[{i}].genotypes[{j}]: weightedSum requires a weight"
                    )));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> SkillManifest {
        SkillManifest {
            schema_version: 1,
            id: "test.skill".into(),
            version: "1.0.0".into(),
            name: "Test".into(),
            description: "d".into(),
            category: "c".into(),
            method: SkillMethod::GenotypeMap,
            reference_deps: vec![],
            variants: vec![VariantRule {
                rsid: "rs1".into(),
                label: None,
                category: None,
                description: None,
                population_frequency: None,
                genotypes: vec![GenotypeOutcome {
                    genotype: "AA".into(),
                    prediction: "p".into(),
                    confidence: 0.5,
                    effect: "e".into(),
                    weight: None,
                }],
            }],
            citations: vec![],
            evidence_tier: EvidenceTier::Community,
            disclaimer: "not medical advice".into(),
        }
    }

    #[test]
    fn valid_manifest_passes() {
        assert!(minimal().validate().is_ok());
    }

    #[test]
    fn rejects_future_schema_version() {
        let mut m = minimal();
        m.schema_version = SCHEMA_VERSION + 1;
        assert!(m.validate().is_err());
    }

    #[test]
    fn rejects_zero_schema_version() {
        let mut m = minimal();
        m.schema_version = 0;
        assert!(m.validate().is_err());
    }

    #[test]
    fn rejects_empty_disclaimer() {
        let mut m = minimal();
        m.disclaimer = "   ".into();
        assert!(m.validate().is_err());
    }

    #[test]
    fn rejects_no_variants() {
        let mut m = minimal();
        m.variants.clear();
        assert!(m.validate().is_err());
    }

    #[test]
    fn rejects_out_of_range_confidence() {
        let mut m = minimal();
        m.variants[0].genotypes[0].confidence = 1.5;
        assert!(m.validate().is_err());
    }

    #[test]
    fn weighted_sum_requires_weight() {
        let mut m = minimal();
        m.method = SkillMethod::WeightedSum;
        assert!(m.validate().is_err());
        m.variants[0].genotypes[0].weight = Some(1.0);
        assert!(m.validate().is_ok());
    }

    #[test]
    fn round_trips_through_json() {
        let m = minimal();
        let json = serde_json::to_string(&m).unwrap();
        let back = SkillManifest::from_json(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn reference_dep_source_keys() {
        assert_eq!(ReferenceDep::GwasCatalog.source_key(), "gwas_catalog");
        assert_eq!(ReferenceDep::Snpedia.source_key(), "snpedia");
    }
}
