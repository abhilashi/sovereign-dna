use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Information about a drug affected by a pharmacogenomic result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrugInfo {
    pub name: String,
    pub category: String,
    pub recommendation: String,
    pub evidence_level: String,
}

/// Pharmacogenomic analysis result for a single gene/enzyme.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PharmaResult {
    pub gene: String,
    pub star_allele: String,
    pub phenotype: String,
    pub affected_drugs: Vec<DrugInfo>,
    pub clinical_actionability: String,
    pub source: String,
}

/// Internal definition for star allele lookups.
struct StarAlleleDef {
    gene: &'static str,
    allele_name: &'static str,
    rsid: &'static str,
    variant_genotype: &'static str, // the genotype indicating this variant
}

const STAR_ALLELE_DEFS: &[StarAlleleDef] = &[
    // CYP2D6
    StarAlleleDef { gene: "CYP2D6", allele_name: "*3", rsid: "rs35742686", variant_genotype: "DEL" },
    StarAlleleDef { gene: "CYP2D6", allele_name: "*4", rsid: "rs3892097", variant_genotype: "AA" },
    StarAlleleDef { gene: "CYP2D6", allele_name: "*5", rsid: "rs5030655", variant_genotype: "DEL" },
    StarAlleleDef { gene: "CYP2D6", allele_name: "*6", rsid: "rs5030656", variant_genotype: "DEL" },
    StarAlleleDef { gene: "CYP2D6", allele_name: "*10", rsid: "rs1065852", variant_genotype: "TT" },

    // CYP2C19
    StarAlleleDef { gene: "CYP2C19", allele_name: "*2", rsid: "rs4244285", variant_genotype: "AA" },
    StarAlleleDef { gene: "CYP2C19", allele_name: "*3", rsid: "rs4986893", variant_genotype: "AA" },
    StarAlleleDef { gene: "CYP2C19", allele_name: "*17", rsid: "rs12248560", variant_genotype: "TT" },

    // CYP2C9
    StarAlleleDef { gene: "CYP2C9", allele_name: "*2", rsid: "rs1799853", variant_genotype: "TT" },
    StarAlleleDef { gene: "CYP2C9", allele_name: "*3", rsid: "rs1057910", variant_genotype: "CC" },

    // CYP3A4
    StarAlleleDef { gene: "CYP3A4", allele_name: "*22", rsid: "rs35599367", variant_genotype: "TT" },

    // CYP1A2
    StarAlleleDef { gene: "CYP1A2", allele_name: "*1F", rsid: "rs762551", variant_genotype: "AA" },
];

struct GeneDrugMapping {
    gene: &'static str,
    drugs: &'static [(&'static str, &'static str, &'static str)], // (name, category, evidence_level)
}

const GENE_DRUG_MAPPINGS: &[GeneDrugMapping] = &[
    GeneDrugMapping {
        gene: "CYP2D6",
        drugs: &[
            ("Codeine", "Pain/Opioid", "1A"),
            ("Tramadol", "Pain/Opioid", "1A"),
            ("Tamoxifen", "Oncology", "1A"),
            ("Fluoxetine", "Antidepressant (SSRI)", "2A"),
            ("Paroxetine", "Antidepressant (SSRI)", "1A"),
            ("Amitriptyline", "Antidepressant (TCA)", "1A"),
            ("Atomoxetine", "ADHD", "1A"),
            ("Metoprolol", "Beta-blocker", "2A"),
        ],
    },
    GeneDrugMapping {
        gene: "CYP2C19",
        drugs: &[
            ("Clopidogrel", "Antiplatelet", "1A"),
            ("Omeprazole", "Proton Pump Inhibitor", "2A"),
            ("Escitalopram", "Antidepressant (SSRI)", "1A"),
            ("Sertraline", "Antidepressant (SSRI)", "2A"),
            ("Voriconazole", "Antifungal", "1A"),
        ],
    },
    GeneDrugMapping {
        gene: "CYP2C9",
        drugs: &[
            ("Warfarin", "Anticoagulant", "1A"),
            ("Phenytoin", "Anticonvulsant", "1A"),
            ("Celecoxib", "NSAID", "2A"),
            ("Ibuprofen", "NSAID", "2B"),
        ],
    },
    GeneDrugMapping {
        gene: "CYP3A4",
        drugs: &[
            ("Tacrolimus", "Immunosuppressant", "1A"),
            ("Simvastatin", "Statin", "2A"),
            ("Midazolam", "Benzodiazepine", "2B"),
        ],
    },
    GeneDrugMapping {
        gene: "CYP1A2",
        drugs: &[
            ("Caffeine", "Stimulant", "2A"),
            ("Clozapine", "Antipsychotic", "2A"),
            ("Theophylline", "Bronchodilator", "2B"),
        ],
    },
];

/// Analyze pharmacogenomics: map user's genotypes to star alleles and metabolizer phenotypes.
pub fn analyze_pharmacogenomics(
    conn: &Connection,
    genome_id: i64,
) -> Result<Vec<PharmaResult>, AppError> {
    // Collect all pharmacogenomic rsids
    let rsids: Vec<&str> = STAR_ALLELE_DEFS.iter().map(|d| d.rsid).collect();

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

    // For each gene, determine star alleles and phenotype
    let genes = ["CYP2D6", "CYP2C19", "CYP2C9", "CYP3A4", "CYP1A2"];
    let mut results = Vec::new();

    for gene in &genes {
        let gene_defs: Vec<&StarAlleleDef> = STAR_ALLELE_DEFS
            .iter()
            .filter(|d| d.gene == *gene)
            .collect();

        // Determine which variant alleles are present
        let mut detected_alleles: Vec<String> = Vec::new();
        let mut has_any_data = false;
        let mut loss_of_function_count = 0;
        let mut gain_of_function = false;

        for def in &gene_defs {
            if let Some(genotype) = user_snps.get(def.rsid) {
                has_any_data = true;

                // Check if genotype matches the variant allele
                let is_homozygous_variant =
                    genotype.to_uppercase() == def.variant_genotype.to_uppercase();
                let is_heterozygous = !is_homozygous_variant
                    && genotype
                        .chars()
                        .any(|c| def.variant_genotype.starts_with(c));

                if is_homozygous_variant {
                    detected_alleles.push(format!("{}/{}", def.allele_name, def.allele_name));
                    // *17 for CYP2C19 is gain-of-function
                    if def.allele_name == "*17" {
                        gain_of_function = true;
                    } else {
                        loss_of_function_count += 2;
                    }
                } else if is_heterozygous {
                    detected_alleles.push(format!("*1/{}", def.allele_name));
                    if def.allele_name == "*17" {
                        gain_of_function = true;
                    } else {
                        loss_of_function_count += 1;
                    }
                }
            }
        }

        if !has_any_data {
            continue;
        }

        let star_allele = if detected_alleles.is_empty() {
            "*1/*1".to_string()
        } else {
            detected_alleles.join(", ")
        };

        // Determine phenotype based on loss/gain of function allele count
        let phenotype = if gain_of_function && loss_of_function_count == 0 {
            "ultra-rapid"
        } else if loss_of_function_count >= 2 {
            "poor"
        } else if loss_of_function_count == 1 {
            "intermediate"
        } else {
            "normal"
        };

        // Get drug mappings for this gene
        let drugs = GENE_DRUG_MAPPINGS
            .iter()
            .find(|m| m.gene == *gene)
            .map(|m| m.drugs)
            .unwrap_or(&[]);

        let affected_drugs: Vec<DrugInfo> = drugs
            .iter()
            .map(|(name, category, evidence)| {
                let recommendation = match phenotype {
                    "poor" => format!(
                        "Consider alternative medication or significant dose reduction for {}",
                        name
                    ),
                    "intermediate" => {
                        format!("Consider dose reduction or monitoring for {}", name)
                    }
                    "ultra-rapid" => format!(
                        "Standard doses may be ineffective; consider dose increase or alternative for {}",
                        name
                    ),
                    _ => format!("Standard dosing expected to be appropriate for {}", name),
                };
                DrugInfo {
                    name: name.to_string(),
                    category: category.to_string(),
                    recommendation,
                    evidence_level: evidence.to_string(),
                }
            })
            .collect();

        let clinical_actionability = match phenotype {
            "poor" | "ultra-rapid" => "high".to_string(),
            "intermediate" => "moderate".to_string(),
            _ => "low".to_string(),
        };

        results.push(PharmaResult {
            gene: gene.to_string(),
            star_allele,
            phenotype: phenotype.to_string(),
            affected_drugs,
            clinical_actionability,
            source: "curated".to_string(),
        });
    }

    // --- ClinVar enrichment for pharmacogenes ---
    if is_reference_ready(conn, "clinvar") {
        let curated_genes: std::collections::HashSet<String> =
            results.iter().map(|r| r.gene.to_uppercase()).collect();

        // Additional pharmacogenes beyond CYP family
        let pharma_genes = [
            "DPYD", "TPMT", "UGT1A1", "VKORC1", "SLCO1B1", "NAT2", "HLA-B",
            "CYP2D6", "CYP2C19", "CYP2C9", "CYP3A4", "CYP1A2",
        ];

        let gene_placeholders: String = pharma_genes.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT DISTINCT a.gene, a.clinical_significance, a.review_status, s.rsid, s.genotype
             FROM annotations a
             INNER JOIN snps s ON a.rsid = s.rsid
             WHERE s.genome_id = ?1
               AND a.gene IN ({})
               AND a.clinical_significance IS NOT NULL
               AND a.clinical_significance != ''",
            gene_placeholders
        );

        let mut stmt = conn.prepare(&sql)?;
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        params.push(Box::new(genome_id));
        for gene in &pharma_genes {
            params.push(Box::new(gene.to_string()));
        }
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let clinvar_rows: Vec<(String, String, Option<String>, String, String)> = stmt
            .query_map(param_refs.as_slice(), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        // Group by gene
        let mut gene_groups: std::collections::HashMap<
            String,
            Vec<(String, Option<String>, String, String)>,
        > = std::collections::HashMap::new();
        for (gene, clin_sig, review_status, rsid, genotype) in clinvar_rows {
            gene_groups
                .entry(gene)
                .or_default()
                .push((clin_sig, review_status, rsid, genotype));
        }

        for (gene, variants) in gene_groups {
            // Skip genes already covered by star allele analysis
            if curated_genes.contains(&gene.to_uppercase()) {
                continue;
            }

            // Use the most severe clinical significance as the phenotype
            let phenotype = variants
                .iter()
                .map(|(sig, _, _, _)| sig.clone())
                .next()
                .unwrap_or_else(|| "unknown".to_string());

            // Determine actionability from review_status
            let clinical_actionability = variants
                .iter()
                .find_map(|(_, rs, _, _)| rs.as_ref())
                .map(|rs| {
                    if rs.contains("practice guideline") || rs.contains("reviewed by expert") {
                        "high"
                    } else if rs.contains("criteria provided") {
                        "moderate"
                    } else {
                        "low"
                    }
                })
                .unwrap_or("low")
                .to_string();

            // Build star_allele string from rsids
            let star_allele = variants
                .iter()
                .map(|(_, _, rsid, genotype)| format!("{}({})", rsid, genotype))
                .collect::<Vec<_>>()
                .join(", ");

            results.push(PharmaResult {
                gene,
                star_allele,
                phenotype,
                affected_drugs: Vec::new(),
                clinical_actionability,
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
