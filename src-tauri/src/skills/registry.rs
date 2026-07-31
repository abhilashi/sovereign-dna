//! Content-addressed local skill registry + distribution (Phase 2.5/2.6).
//!
//! Skills install **without a central server**: they are signed manifests
//! distributed as static files (a local path, a `git`-checked-out file, or a
//! CDN/IPFS URL). The registry:
//!
//! * **content-addresses** every skill by the SHA-256 of its canonical manifest
//!   bytes, so identical skills dedupe and any tampering changes the address;
//! * **verifies the Ed25519 signature against the trust store before storing or
//!   running** a skill (never persists or executes untrusted content);
//! * stores each installed skill as its signed envelope under a flat directory,
//!   named by its content id.
//!
//! Reference-dependency enforcement happens at *run* time (a reference DB may be
//! downloaded after a skill is installed) — see [`SkillRegistry::run`], which
//! delegates to [`crate::skills::engine::evaluate`].
//!
//! The core here is pure/synchronous and unit-tested with a temp directory; the
//! network fetch that turns a CDN/git URL into bytes lives in
//! [`crate::skills`] (`install_from_url`) so this module stays dependency-light.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::engine::{self, GenotypeSource, ReferenceAvailability, SkillOutput};
use super::signing::{self, SignedManifest, TrustStore};

/// Errors from registry operations.
#[derive(Debug)]
pub enum RegistryError {
    Io(String),
    Parse(String),
    Verify(signing::SignError),
    NotFound(String),
    Engine(engine::EngineError),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::Io(m) => write!(f, "registry io error: {m}"),
            RegistryError::Parse(m) => write!(f, "registry parse error: {m}"),
            RegistryError::Verify(e) => write!(f, "skill verification failed: {e}"),
            RegistryError::NotFound(id) => write!(f, "skill not installed: {id}"),
            RegistryError::Engine(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RegistryError {}

impl From<signing::SignError> for RegistryError {
    fn from(e: signing::SignError) -> Self {
        RegistryError::Verify(e)
    }
}

/// The content id of a skill: `sha256-<hex>` over the canonical manifest bytes.
pub fn content_id(signed: &SignedManifest) -> Result<String, RegistryError> {
    let bytes = signing::canonical_bytes(&signed.manifest)
        .map_err(RegistryError::Verify)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest {
        hex.push_str(&format!("{b:02x}"));
    }
    Ok(format!("sha256-{hex}"))
}

/// A skill that is installed on this device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSkill {
    /// Content address (`sha256-...`).
    pub content_id: String,
    /// The skill's own declared id (`org.sovereigndna.traits.core`).
    pub skill_id: String,
    pub version: String,
    pub name: String,
    pub evidence_tier: super::manifest::EvidenceTier,
    /// Hex public key that signed it (already verified as trusted).
    pub public_key: String,
}

impl InstalledSkill {
    fn from_signed(content_id: String, signed: &SignedManifest) -> Self {
        InstalledSkill {
            content_id,
            skill_id: signed.manifest.id.clone(),
            version: signed.manifest.version.clone(),
            name: signed.manifest.name.clone(),
            evidence_tier: signed.manifest.evidence_tier,
            public_key: signed.public_key_hex(),
        }
    }
}

/// A local, content-addressed store of installed skills.
pub struct SkillRegistry {
    root: PathBuf,
    trust: TrustStore,
}

impl SkillRegistry {
    /// Open (creating if needed) a registry rooted at `root`, trusting `trust`.
    pub fn open(root: impl AsRef<Path>, trust: TrustStore) -> Result<Self, RegistryError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root).map_err(|e| RegistryError::Io(e.to_string()))?;
        Ok(Self { root, trust })
    }

    fn path_for(&self, content_id: &str) -> PathBuf {
        self.root.join(format!("{content_id}.skill.json"))
    }

    /// Install an already-parsed signed skill. **Verifies signature + trust
    /// before writing.** Content-addressed, so re-installing identical content is
    /// idempotent.
    pub fn install(&self, signed: &SignedManifest) -> Result<InstalledSkill, RegistryError> {
        // Security gate: refuse to persist anything we would not run.
        signed.verify(&self.trust)?;
        let cid = content_id(signed)?;
        let json = serde_json::to_vec_pretty(signed)
            .map_err(|e| RegistryError::Parse(e.to_string()))?;
        let path = self.path_for(&cid);
        // atomic write: temp + rename
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, &json).map_err(|e| RegistryError::Io(e.to_string()))?;
        fs::rename(&tmp, &path).map_err(|e| RegistryError::Io(e.to_string()))?;
        Ok(InstalledSkill::from_signed(cid, signed))
    }

    /// Install from raw signed-envelope JSON bytes (e.g. a downloaded file).
    pub fn install_from_bytes(&self, bytes: &[u8]) -> Result<InstalledSkill, RegistryError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|e| RegistryError::Parse(e.to_string()))?;
        let signed = SignedManifest::from_json(text)?;
        self.install(&signed)
    }

    /// Install from a local file / git-checked-out path.
    pub fn install_from_file(&self, path: impl AsRef<Path>) -> Result<InstalledSkill, RegistryError> {
        let bytes = fs::read(path.as_ref()).map_err(|e| RegistryError::Io(e.to_string()))?;
        self.install_from_bytes(&bytes)
    }

    /// Load the signed envelope for a content id (does not verify).
    fn load_signed(&self, content_id: &str) -> Result<SignedManifest, RegistryError> {
        let path = self.path_for(content_id);
        if !path.exists() {
            return Err(RegistryError::NotFound(content_id.to_string()));
        }
        let bytes = fs::read(&path).map_err(|e| RegistryError::Io(e.to_string()))?;
        let text = std::str::from_utf8(&bytes).map_err(|e| RegistryError::Parse(e.to_string()))?;
        SignedManifest::from_json(text).map_err(RegistryError::Verify)
    }

    /// Load a skill and re-verify it against the trust store (defence in depth:
    /// the on-disk file could have been swapped since install).
    pub fn load_verified(
        &self,
        content_id: &str,
    ) -> Result<super::manifest::SkillManifest, RegistryError> {
        let signed = self.load_signed(content_id)?;
        // Content id must still match the bytes (detects rename/tamper).
        let actual = self::content_id(&signed)?;
        if actual != content_id {
            return Err(RegistryError::Verify(signing::SignError::BadSignature));
        }
        signed.verify(&self.trust)?;
        Ok(signed.manifest)
    }

    /// List installed skills whose signatures still verify against the trust
    /// store. Files that fail verification are skipped (not silently trusted).
    pub fn list(&self) -> Result<Vec<InstalledSkill>, RegistryError> {
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(|e| RegistryError::Io(e.to_string()))? {
            let entry = entry.map_err(|e| RegistryError::Io(e.to_string()))?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with(".skill.json") {
                continue;
            }
            let bytes = match fs::read(entry.path()) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let Ok(text) = std::str::from_utf8(&bytes) else {
                continue;
            };
            let Ok(signed) = SignedManifest::from_json(text) else {
                continue;
            };
            if signed.verify(&self.trust).is_err() {
                continue;
            }
            if let Ok(cid) = content_id(&signed) {
                out.push(InstalledSkill::from_signed(cid, &signed));
            }
        }
        out.sort_by(|a, b| a.skill_id.cmp(&b.skill_id));
        Ok(out)
    }

    /// Remove an installed skill.
    pub fn remove(&self, content_id: &str) -> Result<(), RegistryError> {
        let path = self.path_for(content_id);
        if !path.exists() {
            return Err(RegistryError::NotFound(content_id.to_string()));
        }
        fs::remove_file(&path).map_err(|e| RegistryError::Io(e.to_string()))
    }

    /// Verify, then run, an installed skill against a genome. Reference-dep
    /// availability is enforced here (run time), not at install time.
    pub fn run<G: GenotypeSource, R: ReferenceAvailability>(
        &self,
        content_id: &str,
        genome: &G,
        refs: &R,
    ) -> Result<SkillOutput, RegistryError> {
        let manifest = self.load_verified(content_id)?;
        engine::evaluate(&manifest, genome, refs).map_err(RegistryError::Engine)
    }

    pub fn trust(&self) -> &TrustStore {
        &self.trust
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::engine::AllReferencesReady;
    use crate::skills::manifest::{
        EvidenceTier, GenotypeOutcome, SkillManifest, SkillMethod, VariantRule,
    };
    use ed25519_dalek::SigningKey;
    use std::collections::HashMap;

    struct Mem(HashMap<String, String>);
    impl GenotypeSource for Mem {
        fn genotype(&self, rsid: &str) -> Option<String> {
            self.0.get(rsid).cloned()
        }
    }

    fn manifest(id: &str, pred: &str) -> SkillManifest {
        SkillManifest {
            schema_version: 1,
            id: id.into(),
            version: "1.0.0".into(),
            name: "Test".into(),
            description: "d".into(),
            category: "c".into(),
            method: SkillMethod::GenotypeMap,
            reference_deps: vec![],
            variants: vec![VariantRule {
                rsid: "rs1".into(),
                label: Some("L".into()),
                category: None,
                description: None,
                population_frequency: None,
                genotypes: vec![GenotypeOutcome {
                    genotype: "AA".into(),
                    prediction: pred.into(),
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

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("skillreg-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        p
    }

    fn trust_for(k: &SigningKey) -> TrustStore {
        let mut t = TrustStore::new();
        t.add(k.verifying_key().to_bytes());
        t
    }

    #[test]
    fn content_id_is_stable_and_content_dependent() {
        let k = key(1);
        let a = SignedManifest::create(manifest("x", "p1"), &k).unwrap();
        let a2 = SignedManifest::create(manifest("x", "p1"), &k).unwrap();
        let b = SignedManifest::create(manifest("x", "p2"), &k).unwrap();
        assert_eq!(content_id(&a).unwrap(), content_id(&a2).unwrap());
        assert_ne!(content_id(&a).unwrap(), content_id(&b).unwrap());
        assert!(content_id(&a).unwrap().starts_with("sha256-"));
    }

    #[test]
    fn install_rejects_untrusted_key() {
        let reg = SkillRegistry::open(tmpdir("untrusted"), TrustStore::new()).unwrap();
        let signed = SignedManifest::create(manifest("x", "p"), &key(1)).unwrap();
        assert!(reg.install(&signed).is_err());
        assert_eq!(reg.list().unwrap().len(), 0);
    }

    #[test]
    fn install_list_run_roundtrip() {
        let k = key(2);
        let reg = SkillRegistry::open(tmpdir("roundtrip"), trust_for(&k)).unwrap();
        let signed = SignedManifest::create(manifest("org.x.demo", "hello"), &k).unwrap();
        let installed = reg.install(&signed).unwrap();
        assert_eq!(installed.skill_id, "org.x.demo");

        let list = reg.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].content_id, installed.content_id);

        let genome = Mem(HashMap::from([("rs1".to_string(), "AA".to_string())]));
        let out = reg.run(&installed.content_id, &genome, &AllReferencesReady).unwrap();
        assert_eq!(out.findings[0].prediction, "hello");

        reg.remove(&installed.content_id).unwrap();
        assert_eq!(reg.list().unwrap().len(), 0);
    }

    #[test]
    fn install_is_idempotent_content_addressed() {
        let k = key(3);
        let reg = SkillRegistry::open(tmpdir("idem"), trust_for(&k)).unwrap();
        let signed = SignedManifest::create(manifest("x", "p"), &k).unwrap();
        let a = reg.install(&signed).unwrap();
        let b = reg.install(&signed).unwrap();
        assert_eq!(a.content_id, b.content_id);
        assert_eq!(reg.list().unwrap().len(), 1);
    }

    #[test]
    fn install_from_bytes_works() {
        let k = key(4);
        let reg = SkillRegistry::open(tmpdir("bytes"), trust_for(&k)).unwrap();
        let signed = SignedManifest::create(manifest("x", "p"), &k).unwrap();
        let bytes = serde_json::to_vec(&signed).unwrap();
        let installed = reg.install_from_bytes(&bytes).unwrap();
        assert!(reg.load_verified(&installed.content_id).is_ok());
    }

    #[test]
    fn tampered_file_fails_load_verified() {
        let k = key(5);
        let dir = tmpdir("tamper");
        let reg = SkillRegistry::open(&dir, trust_for(&k)).unwrap();
        let signed = SignedManifest::create(manifest("x", "orig"), &k).unwrap();
        let installed = reg.install(&signed).unwrap();
        // Corrupt the stored file in place (change the prediction).
        let path = reg.path_for(&installed.content_id);
        let corrupted = fs::read_to_string(&path).unwrap().replace("orig", "evil");
        fs::write(&path, corrupted).unwrap();
        // Both the content-address check and the signature check should reject it.
        assert!(reg.load_verified(&installed.content_id).is_err());
        // ...and it is filtered out of listings.
        assert_eq!(reg.list().unwrap().len(), 0);
    }
}
