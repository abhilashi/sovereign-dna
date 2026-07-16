//! Signed skill manifests (Phase 2.4).
//!
//! Skills are untrusted content that will be distributed as static files
//! (Phase 2.5/2.6) from git/CDN/peers — a malicious "skill" could try to lie
//! about what it does. Before a skill runs we therefore require a valid
//! **Ed25519 signature from a trusted key** over the manifest's canonical bytes,
//! and we re-check that its declared reference dependencies are known.
//!
//! Signing is *detached from formatting*: we sign the canonical re-serialisation
//! of the parsed manifest, not the raw file bytes, so reformatting / re-indenting
//! a manifest does not invalidate its signature (only changing its meaning does).

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::manifest::SkillManifest;

/// Errors from signing / verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignError {
    Serialize(String),
    Encoding(String),
    BadSignature,
    UntrustedKey(String),
    InvalidManifest(String),
}

impl std::fmt::Display for SignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignError::Serialize(m) => write!(f, "canonicalisation failed: {m}"),
            SignError::Encoding(m) => write!(f, "bad hex encoding: {m}"),
            SignError::BadSignature => write!(f, "signature does not verify"),
            SignError::UntrustedKey(k) => write!(f, "public key not in trust store: {k}"),
            SignError::InvalidManifest(m) => write!(f, "invalid manifest: {m}"),
        }
    }
}

impl std::error::Error for SignError {}

/// Deterministic byte representation of a manifest used for signing/verifying.
///
/// The manifest is composed entirely of structs and vecs (no unordered maps), so
/// `serde_json` emits identical bytes for identical content regardless of how the
/// source file was formatted.
pub fn canonical_bytes(m: &SkillManifest) -> Result<Vec<u8>, SignError> {
    canonical_json(m)
}

/// Generic canonical JSON bytes for any signable value composed of structs/vecs
/// (no unordered maps), so `serde_json` emits identical bytes for identical
/// content. Reused by the Phase-3.9 agent-definition sharing layer so agents are
/// signed with exactly the same scheme as skills.
pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, SignError> {
    serde_json::to_vec(value).map_err(|e| SignError::Serialize(e.to_string()))
}

/// Hex-encode bytes (public for reuse by the agent-sharing layer).
pub fn hex_encode(bytes: &[u8]) -> String {
    to_hex(bytes)
}

/// Hex-decode (public for reuse by the agent-sharing layer).
pub fn hex_decode(s: &str) -> Result<Vec<u8>, SignError> {
    from_hex(s)
}

/// Sign the canonical bytes of `value`, returning `(signature_hex, public_key_hex)`.
pub fn sign_canonical<T: Serialize>(
    value: &T,
    key: &SigningKey,
) -> Result<(String, String), SignError> {
    let bytes = canonical_json(value)?;
    let sig: Signature = key.sign(&bytes);
    Ok((to_hex(&sig.to_bytes()), to_hex(key.verifying_key().as_bytes())))
}

/// Verify a detached signature (hex) over the canonical bytes of `value` for a
/// given public key (hex). Does not consult any trust store.
pub fn verify_canonical<T: Serialize>(
    value: &T,
    signature_hex: &str,
    public_key_hex: &str,
) -> Result<(), SignError> {
    let vk_raw = from_hex(public_key_hex)?;
    let vk_arr: [u8; 32] = vk_raw
        .try_into()
        .map_err(|_| SignError::Encoding("public key must be 32 bytes".into()))?;
    let vk = VerifyingKey::from_bytes(&vk_arr).map_err(|e| SignError::Encoding(e.to_string()))?;
    let sig_raw = from_hex(signature_hex)?;
    let sig_arr: [u8; 64] = sig_raw
        .try_into()
        .map_err(|_| SignError::Encoding("signature must be 64 bytes".into()))?;
    let sig = Signature::from_bytes(&sig_arr);
    let bytes = canonical_json(value)?;
    vk.verify(&bytes, &sig).map_err(|_| SignError::BadSignature)
}

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn from_hex(s: &str) -> Result<Vec<u8>, SignError> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return Err(SignError::Encoding("odd length".into()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| SignError::Encoding(e.to_string()))
        })
        .collect()
}

/// A manifest plus a detached signature and the key that produced it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedManifest {
    pub manifest: SkillManifest,
    /// Ed25519 signature (64 bytes, hex) over [`canonical_bytes`] of `manifest`.
    pub signature: String,
    /// Ed25519 public key (32 bytes, hex) that produced the signature.
    pub public_key: String,
}

impl SignedManifest {
    /// Produce a signed envelope for `manifest` using `key`.
    pub fn create(manifest: SkillManifest, key: &SigningKey) -> Result<Self, SignError> {
        manifest
            .validate()
            .map_err(|e| SignError::InvalidManifest(e.0))?;
        let bytes = canonical_bytes(&manifest)?;
        let sig: Signature = key.sign(&bytes);
        Ok(SignedManifest {
            manifest,
            signature: to_hex(&sig.to_bytes()),
            public_key: to_hex(key.verifying_key().as_bytes()),
        })
    }

    /// Parse a signed envelope from JSON (does **not** verify — call [`verify`]).
    pub fn from_json(s: &str) -> Result<Self, SignError> {
        serde_json::from_str(s).map_err(|e| SignError::Serialize(e.to_string()))
    }

    fn verifying_key(&self) -> Result<VerifyingKey, SignError> {
        let raw = from_hex(&self.public_key)?;
        let arr: [u8; 32] = raw
            .try_into()
            .map_err(|_| SignError::Encoding("public key must be 32 bytes".into()))?;
        VerifyingKey::from_bytes(&arr).map_err(|e| SignError::Encoding(e.to_string()))
    }

    fn signature(&self) -> Result<Signature, SignError> {
        let raw = from_hex(&self.signature)?;
        let arr: [u8; 64] = raw
            .try_into()
            .map_err(|_| SignError::Encoding("signature must be 64 bytes".into()))?;
        Ok(Signature::from_bytes(&arr))
    }

    /// Verify the signature is valid for the embedded public key (ignoring trust).
    pub fn verify_signature(&self) -> Result<(), SignError> {
        self.manifest
            .validate()
            .map_err(|e| SignError::InvalidManifest(e.0))?;
        let vk = self.verifying_key()?;
        let sig = self.signature()?;
        let bytes = canonical_bytes(&self.manifest)?;
        vk.verify(&bytes, &sig).map_err(|_| SignError::BadSignature)
    }

    /// The public key as canonical lowercase hex.
    pub fn public_key_hex(&self) -> String {
        self.public_key.trim().to_lowercase()
    }

    /// Full gate applied before a skill may run: the signing key must be trusted
    /// **and** the signature must verify **and** the manifest must be valid.
    /// Returns the verified manifest on success.
    pub fn verify(&self, trust: &TrustStore) -> Result<&SkillManifest, SignError> {
        if !trust.contains_hex(&self.public_key_hex()) {
            return Err(SignError::UntrustedKey(self.public_key_hex()));
        }
        self.verify_signature()?;
        Ok(&self.manifest)
    }
}

/// The set of Ed25519 public keys whose skills this device will run.
#[derive(Debug, Clone, Default)]
pub struct TrustStore {
    keys: HashSet<[u8; 32]>,
}

impl TrustStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a trusted key from 32 raw bytes.
    pub fn add(&mut self, key: [u8; 32]) {
        self.keys.insert(key);
    }

    /// Add a trusted key from hex; errors if malformed.
    pub fn add_hex(&mut self, hex: &str) -> Result<(), SignError> {
        let raw = from_hex(hex)?;
        let arr: [u8; 32] = raw
            .try_into()
            .map_err(|_| SignError::Encoding("public key must be 32 bytes".into()))?;
        self.keys.insert(arr);
        Ok(())
    }

    /// Whether a public key (hex) is trusted. Public so the agent-sharing layer
    /// (Phase 3.9) can reuse the same trust store for signed agent definitions.
    pub fn trusts_hex(&self, hex: &str) -> bool {
        self.contains_hex(hex)
    }

    fn contains_hex(&self, hex: &str) -> bool {
        match from_hex(hex) {
            Ok(raw) => match <[u8; 32]>::try_from(raw) {
                Ok(arr) => self.keys.contains(&arr),
                Err(_) => false,
            },
            Err(_) => false,
        }
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::manifest::{
        EvidenceTier, GenotypeOutcome, SkillMethod, VariantRule,
    };

    fn manifest(id: &str) -> SkillManifest {
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

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    #[test]
    fn hex_round_trips() {
        let bytes = [0u8, 1, 15, 16, 255, 128];
        assert_eq!(from_hex(&to_hex(&bytes)).unwrap(), bytes);
    }

    #[test]
    fn sign_then_verify_signature_ok() {
        let signed = SignedManifest::create(manifest("a"), &key(7)).unwrap();
        assert!(signed.verify_signature().is_ok());
    }

    #[test]
    fn tampered_manifest_fails_signature() {
        let mut signed = SignedManifest::create(manifest("a"), &key(7)).unwrap();
        signed.manifest.variants[0].genotypes[0].confidence = 0.9; // tamper
        assert_eq!(signed.verify_signature().unwrap_err(), SignError::BadSignature);
    }

    #[test]
    fn wrong_key_signature_fails() {
        let mut signed = SignedManifest::create(manifest("a"), &key(7)).unwrap();
        // swap in an unrelated key's public part but keep old signature
        signed.public_key = to_hex(key(9).verifying_key().as_bytes());
        assert_eq!(signed.verify_signature().unwrap_err(), SignError::BadSignature);
    }

    #[test]
    fn untrusted_key_is_rejected_by_verify() {
        let signed = SignedManifest::create(manifest("a"), &key(7)).unwrap();
        let trust = TrustStore::new(); // empty
        assert!(matches!(
            signed.verify(&trust).unwrap_err(),
            SignError::UntrustedKey(_)
        ));
    }

    #[test]
    fn trusted_key_passes_verify() {
        let k = key(7);
        let signed = SignedManifest::create(manifest("a"), &k).unwrap();
        let mut trust = TrustStore::new();
        trust.add(k.verifying_key().to_bytes());
        assert!(signed.verify(&trust).is_ok());
        assert_eq!(trust.len(), 1);
    }

    #[test]
    fn reformatting_does_not_break_signature() {
        // Sign, serialise the envelope to JSON with different whitespace, parse
        // back, and re-verify: signatures are over canonical bytes, not raw text.
        let signed = SignedManifest::create(manifest("a"), &key(3)).unwrap();
        let pretty = serde_json::to_string_pretty(&signed).unwrap();
        let back = SignedManifest::from_json(&pretty).unwrap();
        assert!(back.verify_signature().is_ok());
    }

    #[test]
    fn add_hex_round_trips_into_trust() {
        let k = key(5);
        let pk_hex = to_hex(k.verifying_key().as_bytes());
        let mut trust = TrustStore::new();
        trust.add_hex(&pk_hex).unwrap();
        let signed = SignedManifest::create(manifest("a"), &k).unwrap();
        assert!(signed.verify(&trust).is_ok());
    }
}
