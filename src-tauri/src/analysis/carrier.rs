use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// A specific variant checked for carrier status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarrierVariant {
    pub rsid: String,
    pub genotype: String,
    pub pathogenic_allele: String,
    pub is_carrier: bool,
}

/// Carrier status result for a single condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarrierResult {
    pub condition: String,
    pub gene: String,
    pub status: String,
    pub variants_checked: Vec<CarrierVariant>,
    pub inheritance_pattern: String,
    pub description: String,
    pub source: String,
}

/// Definition for a carrier screening variant.
struct CarrierDef {
    gene: &'static str,
    condition: &'static str,
    inheritance: &'static str,
    description: &'static str,
    variants: &'static [CarrierVariantDef],
}

struct CarrierVariantDef {
    rsid: &'static str,
    pathogenic_allele: &'static str,
}

const CARRIER_DEFS: &[CarrierDef] = &[
    CarrierDef {
        gene: "CFTR",
        condition: "Cystic Fibrosis",
        inheritance: "autosomal recessive",
        description: "Cystic fibrosis is a genetic disorder affecting the lungs, pancreas, and other organs. It is caused by mutations in the CFTR gene that affect chloride ion transport.",
        variants: &[
            CarrierVariantDef { rsid: "rs75961395", pathogenic_allele: "T" },   // F508del related
            CarrierVariantDef { rsid: "rs78655421", pathogenic_allele: "A" },   // G542X related
            CarrierVariantDef { rsid: "rs113993960", pathogenic_allele: "DEL" }, // delta F508
            CarrierVariantDef { rsid: "rs74767530", pathogenic_allele: "A" },   // G551D related
        ],
    },
    CarrierDef {
        gene: "HBB",
        condition: "Sickle Cell Disease",
        inheritance: "autosomal recessive",
        description: "Sickle cell disease is caused by a mutation in the HBB gene, leading to abnormal hemoglobin that deforms red blood cells into a sickle shape.",
        variants: &[
            CarrierVariantDef { rsid: "rs334", pathogenic_allele: "T" },         // HbS mutation
            CarrierVariantDef { rsid: "rs33930165", pathogenic_allele: "A" },    // HbC mutation
        ],
    },
    CarrierDef {
        gene: "HEXA",
        condition: "Tay-Sachs Disease",
        inheritance: "autosomal recessive",
        description: "Tay-Sachs disease is a fatal genetic disorder caused by HEXA gene mutations, leading to destruction of nerve cells in the brain and spinal cord.",
        variants: &[
            CarrierVariantDef { rsid: "rs76173977", pathogenic_allele: "A" },
            CarrierVariantDef { rsid: "rs121907972", pathogenic_allele: "C" },
        ],
    },
    CarrierDef {
        gene: "SMN1",
        condition: "Spinal Muscular Atrophy",
        inheritance: "autosomal recessive",
        description: "SMA is a genetic condition affecting motor neurons, caused by mutations in the SMN1 gene leading to progressive muscle weakness.",
        variants: &[
            CarrierVariantDef { rsid: "rs1554286660", pathogenic_allele: "DEL" },
            CarrierVariantDef { rsid: "rs143838139", pathogenic_allele: "A" },
        ],
    },
    CarrierDef {
        gene: "GJB2",
        condition: "Non-syndromic Hearing Loss",
        inheritance: "autosomal recessive",
        description: "Mutations in the GJB2 gene (connexin 26) are the most common cause of hereditary non-syndromic hearing loss.",
        variants: &[
            CarrierVariantDef { rsid: "rs80338939", pathogenic_allele: "DEL" },  // 35delG
            CarrierVariantDef { rsid: "rs72474224", pathogenic_allele: "T" },    // V37I related
            CarrierVariantDef { rsid: "rs80338943", pathogenic_allele: "T" },    // 167delT related
        ],
    },
    CarrierDef {
        gene: "PAH",
        condition: "Phenylketonuria (PKU)",
        inheritance: "autosomal recessive",
        description: "PKU is a metabolic disorder caused by PAH gene mutations that prevent proper breakdown of the amino acid phenylalanine.",
        variants: &[
            CarrierVariantDef { rsid: "rs5030841", pathogenic_allele: "A" },     // R408W related
            CarrierVariantDef { rsid: "rs5030849", pathogenic_allele: "T" },
            CarrierVariantDef { rsid: "rs5030858", pathogenic_allele: "G" },
        ],
    },
];

/// Analyze carrier status for common recessive genetic conditions.
pub fn analyze_carrier_status(
    conn: &Connection,
    genome_id: i64,
) -> Result<Vec<CarrierResult>, AppError> {
    // Collect all rsids we need
    let all_rsids: Vec<&str> = CARRIER_DEFS
        .iter()
        .flat_map(|d| d.variants.iter().map(|v| v.rsid))
        .collect();

    let placeholders: String = all_rsids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT rsid, genotype FROM snps WHERE genome_id = ?1 AND rsid IN ({})",
        placeholders
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    params.push(Box::new(genome_id));
    for rsid in &all_rsids {
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

    for def in CARRIER_DEFS {
        let mut variants_checked = Vec::new();
        let mut carrier_count = 0;
        let mut affected_count = 0;
        let mut total_checked = 0;

        for variant_def in def.variants {
            if let Some(genotype) = user_snps.get(variant_def.rsid) {
                total_checked += 1;

                let pathogenic_count = genotype
                    .chars()
                    .filter(|c| {
                        c.to_string()
                            .eq_ignore_ascii_case(variant_def.pathogenic_allele)
                    })
                    .count();

                let is_carrier = pathogenic_count == 1;
                let is_affected = pathogenic_count >= 2;

                if is_carrier {
                    carrier_count += 1;
                }
                if is_affected {
                    affected_count += 1;
                }

                variants_checked.push(CarrierVariant {
                    rsid: variant_def.rsid.to_string(),
                    genotype: genotype.clone(),
                    pathogenic_allele: variant_def.pathogenic_allele.to_string(),
                    is_carrier: is_carrier || is_affected,
                });
            }
        }

        // Determine overall status
        let status = if affected_count > 0 {
            "affected"
        } else if carrier_count > 0 {
            "carrier"
        } else {
            "not_carrier"
        };

        // Include even if no variants were found (to show what was checked)
        results.push(CarrierResult {
            condition: def.condition.to_string(),
            gene: def.gene.to_string(),
            status: status.to_string(),
            variants_checked,
            inheritance_pattern: def.inheritance.to_string(),
            description: if total_checked > 0 {
                def.description.to_string()
            } else {
                format!(
                    "{}. Note: No variants for this gene were found in your data file.",
                    def.description
                )
            },
            source: "curated".to_string(),
        });
    }

    // Collect conditions already covered for deduplication
    let curated_conditions: std::collections::HashSet<String> = results
        .iter()
        .map(|r| r.condition.to_lowercase())
        .collect();
    let curated_genes: std::collections::HashSet<String> = results
        .iter()
        .map(|r| r.gene.to_uppercase())
        .collect();

    // --- ClinVar enrichment for carrier status ---
    if is_reference_ready(conn, "clinvar") {
        let mut clinvar_stmt = conn.prepare(
            "SELECT a.gene, a.clinical_significance, a.condition, s.rsid, s.genotype
             FROM annotations a
             INNER JOIN snps s ON a.rsid = s.rsid
             WHERE s.genome_id = ?1
               AND a.clinical_significance LIKE '%pathogenic%'"
        )?;

        let clinvar_rows: Vec<(String, String, String, String, String)> = clinvar_stmt
            .query_map([genome_id], |row| {
                Ok((
                    row.get::<_, String>(0).unwrap_or_default(),
                    row.get::<_, String>(1).unwrap_or_default(),
                    row.get::<_, String>(2).unwrap_or_default(),
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        // Group by gene
        let mut gene_groups: std::collections::HashMap<
            String,
            Vec<(String, String, String, String)>,
        > = std::collections::HashMap::new();
        for (gene, clin_sig, condition, rsid, genotype) in clinvar_rows {
            if gene.is_empty() {
                continue;
            }
            // Skip genes already covered by curated results
            if curated_genes.contains(&gene.to_uppercase()) {
                continue;
            }
            gene_groups
                .entry(gene)
                .or_default()
                .push((clin_sig, condition, rsid, genotype));
        }

        for (gene, variants) in gene_groups {
            // Determine carrier status by checking genotypes
            let mut is_carrier = false;
            let mut is_affected = false;
            let mut variants_checked = Vec::new();
            let mut condition_name = String::new();

            for (clin_sig, condition, rsid, genotype) in &variants {
                if condition_name.is_empty() && !condition.is_empty() {
                    condition_name = condition.clone();
                }

                // Check if heterozygous (carrier) or homozygous (potentially affected)
                let chars: Vec<char> = genotype.chars().collect();
                let is_het = chars.len() == 2 && chars[0] != chars[1];
                let is_hom_alt = chars.len() == 2 && chars[0] == chars[1] && chars[0] != 'N';

                if is_het {
                    is_carrier = true;
                }
                if is_hom_alt {
                    is_affected = true;
                }

                variants_checked.push(CarrierVariant {
                    rsid: rsid.clone(),
                    genotype: genotype.clone(),
                    pathogenic_allele: clin_sig.clone(),
                    is_carrier: is_het || is_hom_alt,
                });
            }

            if condition_name.is_empty() {
                condition_name = format!("{}-related condition", gene);
            }

            if curated_conditions.contains(&condition_name.to_lowercase()) {
                continue;
            }

            let status = if is_affected {
                "affected"
            } else if is_carrier {
                "carrier"
            } else {
                "not_carrier"
            };

            results.push(CarrierResult {
                condition: condition_name,
                gene,
                status: status.to_string(),
                variants_checked,
                inheritance_pattern: "autosomal recessive".to_string(),
                description: "Pathogenic variant identified via ClinVar annotation.".to_string(),
                source: "clinvar".to_string(),
            });
        }
    }

    Ok(results)
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
