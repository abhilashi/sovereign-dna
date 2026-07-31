//! Skill runtime (Phase 2).
//!
//! Turns the app's historically hardcoded analysis modules into a declarative,
//! versioned, extensible **skill** system. A skill is described by a
//! [`manifest::SkillManifest`] and executed by the [`engine`] against a genome.
//!
//! Sub-phases (see the 4-phase spec §3):
//! * 2.1 — [`manifest`]: versioned skill schema (inputs, reference deps, method,
//!   citations, evidence tier, disclaimer).
//! * 2.2 — [`engine`]: declarative interpreter (no native code execution) plus
//!   the built-in migrated skills below.
//!
//! The built-in [`TRAITS_CORE`] manifest is the first hardcoded module migrated
//! to the new format, proving the pipeline end-to-end.

pub mod engine;
pub mod manifest;

use manifest::SkillManifest;

/// The core traits panel, migrated from the old `analysis::traits` const tables.
pub const TRAITS_CORE_JSON: &str = include_str!("manifests/traits-core.json");

/// Parse and validate the built-in core-traits manifest.
///
/// Panics only if the *bundled* manifest is malformed, which a unit test guards
/// against, so this is infallible in practice.
pub fn traits_core_manifest() -> SkillManifest {
    SkillManifest::from_json(TRAITS_CORE_JSON)
        .expect("bundled traits-core manifest must be valid")
}

/// All manifests shipped inside the binary. The registry (Phase 2.5/2.6) will
/// additionally load installed skills from disk.
pub fn builtin_manifests() -> Vec<SkillManifest> {
    vec![traits_core_manifest()]
}

// The SQLite adapters need `rusqlite`, which is always present in the real crate
// but not in the lightweight test harness. They are unconditionally compiled in
// the app (rusqlite is a hard dependency); the module is separated only for
// readability.
mod db_source {
    use std::collections::HashMap;

    use rusqlite::Connection;

    use super::engine::{GenotypeSource, ReferenceAvailability};
    use super::manifest::ReferenceDep;

    /// A [`GenotypeSource`] backed by the local SQLite `snps` table.
    ///
    /// Genotypes for the requested rsIDs are batch-loaded once up front (a single
    /// `IN (...)` query) so the engine never touches the database mid-evaluation.
    pub struct SqliteGenotypeSource {
        genotypes: HashMap<String, String>,
    }

    impl SqliteGenotypeSource {
        /// Load the genotypes for `rsids` in `genome_id`.
        pub fn load(
            conn: &Connection,
            genome_id: i64,
            rsids: &[String],
        ) -> Result<Self, rusqlite::Error> {
            let mut genotypes = HashMap::new();
            if rsids.is_empty() {
                return Ok(Self { genotypes });
            }
            let placeholders = rsids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT rsid, genotype FROM snps WHERE genome_id = ?1 AND rsid IN ({placeholders})"
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            params.push(Box::new(genome_id));
            for r in rsids {
                params.push(Box::new(r.clone()));
            }
            let refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
            let rows = stmt.query_map(refs.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for r in rows.flatten() {
                genotypes.insert(r.0, r.1);
            }
            Ok(Self { genotypes })
        }
    }

    impl GenotypeSource for SqliteGenotypeSource {
        fn genotype(&self, rsid: &str) -> Option<String> {
            self.genotypes.get(rsid).cloned()
        }
    }

    /// [`ReferenceAvailability`] backed by the `reference_status` table.
    pub struct DbReferenceAvailability<'a> {
        conn: &'a Connection,
    }

    impl<'a> DbReferenceAvailability<'a> {
        pub fn new(conn: &'a Connection) -> Self {
            Self { conn }
        }
    }

    impl ReferenceAvailability for DbReferenceAvailability<'_> {
        fn is_ready(&self, dep: ReferenceDep) -> bool {
            self.conn
                .query_row(
                    "SELECT status FROM reference_status WHERE source = ?1",
                    [dep.source_key()],
                    |row| row.get::<_, String>(0),
                )
                .map(|status| status == "ready")
                .unwrap_or(false)
        }
    }
}

pub use db_source::{DbReferenceAvailability, SqliteGenotypeSource};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_traits_manifest_is_valid() {
        let m = traits_core_manifest();
        assert_eq!(m.id, "org.sovereigndna.traits.core");
        assert_eq!(m.variants.len(), 10);
        assert!(m.validate().is_ok());
    }

    #[test]
    fn builtins_enumerated() {
        assert_eq!(builtin_manifests().len(), 1);
    }
}
