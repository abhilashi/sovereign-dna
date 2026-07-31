//! Agent sharing — export/import agent **definitions only** (Phase 3.9).
//!
//! This is the on-ramp to a collective network (Phase 4): users share *methods*
//! (how to analyse), never *data* (their genome). An [`AgentDefinition`] contains
//! no genome data by construction, and [`assert_shareable`] additionally proves
//! that every identifier it carries is a **public** rsID/coordinate — so an
//! exported agent can never smuggle a genotype off the device.
//!
//! Sharing reuses the **exact Phase-2 machinery**: Ed25519 signatures over
//! canonical bytes ([`crate::skills::signing`]), the same [`TrustStore`], and the
//! same content-addressing (`sha256-…`, [`crate::skills::registry::content_address`]).
//! An imported agent is verified (signature + trust + shareability) before it is
//! ever saved or run.

use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};

use super::definition::AgentDefinition;
use super::ledger::is_public_identifier;
use crate::skills::registry::content_address;
use crate::skills::signing::{self, SignError, TrustStore};

/// Errors from agent export/import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShareError {
    Invalid(String),
    NotShareable(String),
    Sign(SignError),
    Untrusted(String),
}

impl std::fmt::Display for ShareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShareError::Invalid(m) => write!(f, "invalid agent definition: {m}"),
            ShareError::NotShareable(m) => write!(f, "agent is not shareable: {m}"),
            ShareError::Sign(e) => write!(f, "signature error: {e}"),
            ShareError::Untrusted(k) => write!(f, "signing key not trusted: {k}"),
        }
    }
}

impl std::error::Error for ShareError {}

impl From<SignError> for ShareError {
    fn from(e: SignError) -> Self {
        ShareError::Sign(e)
    }
}

/// Prove a definition is safe to share: it must validate **and** every rsID it
/// scopes must be a public identifier (never a genotype). Structurally the type
/// cannot hold a genotype, but this makes the guarantee explicit and testable.
pub fn assert_shareable(def: &AgentDefinition) -> Result<(), ShareError> {
    def.validate().map_err(|e| ShareError::Invalid(e.0))?;
    for rsid in &def.data_scope.rsids {
        if !is_public_identifier(rsid) {
            return Err(ShareError::NotShareable(format!(
                "data scope rsID '{rsid}' is not a public identifier"
            )));
        }
    }
    Ok(())
}

/// A shareable, signed agent **definition** (no genome data).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedAgentDefinition {
    pub definition: AgentDefinition,
    /// Ed25519 signature (hex) over the canonical bytes of `definition`.
    pub signature: String,
    /// Ed25519 public key (hex) that produced the signature.
    pub public_key: String,
}

impl SignedAgentDefinition {
    /// Sign a shareable definition with `key`.
    pub fn create(definition: AgentDefinition, key: &SigningKey) -> Result<Self, ShareError> {
        assert_shareable(&definition)?;
        let (signature, public_key) = signing::sign_canonical(&definition, key)?;
        Ok(SignedAgentDefinition {
            definition,
            signature,
            public_key,
        })
    }

    /// Parse from JSON (does not verify — call [`verify_signature`](Self::verify_signature)).
    pub fn from_json(s: &str) -> Result<Self, ShareError> {
        serde_json::from_str(s)
            .map_err(|e| ShareError::Sign(SignError::Serialize(e.to_string())))
    }

    /// Serialise to pretty JSON for distribution as a static file.
    pub fn to_json(&self) -> Result<String, ShareError> {
        serde_json::to_string_pretty(self)
            .map_err(|e| ShareError::Sign(SignError::Serialize(e.to_string())))
    }

    /// The content address (`sha256-…`) of the signed definition's canonical
    /// bytes — reuses the Phase-2 content-addressing scheme.
    pub fn content_id(&self) -> Result<String, ShareError> {
        let bytes = signing::canonical_json(&self.definition)?;
        Ok(content_address(&bytes))
    }

    pub fn public_key_hex(&self) -> String {
        self.public_key.trim().to_lowercase()
    }

    /// Verify the signature is valid for the embedded key **and** the definition
    /// is shareable (no genome data). Ignores trust.
    pub fn verify_signature(&self) -> Result<(), ShareError> {
        assert_shareable(&self.definition)?;
        signing::verify_canonical(&self.definition, &self.signature, &self.public_key)?;
        Ok(())
    }

    /// Full import gate: the signing key must be trusted **and** the signature
    /// must verify **and** the definition must be shareable. Returns the verified
    /// definition on success.
    pub fn verify(&self, trust: &TrustStore) -> Result<&AgentDefinition, ShareError> {
        if !trust.trusts_hex(&self.public_key_hex()) {
            return Err(ShareError::Untrusted(self.public_key_hex()));
        }
        self.verify_signature()?;
        Ok(&self.definition)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::definition::{DataScope, LlmConfig, Trigger};

    fn def(id: &str) -> AgentDefinition {
        AgentDefinition {
            schema_version: 1,
            id: id.into(),
            version: "1.0.0".into(),
            name: "Shareable".into(),
            description: "d".into(),
            skill_ids: vec!["org.sovereigndna.traits.core".into()],
            data_scope: DataScope {
                rsids: vec!["rs429358".into()],
                topics: vec!["APOE".into()],
            },
            llm: LlmConfig::default(),
            trigger: Trigger::Interval { every_hours: 168 },
            template_id: None,
            instructions: "watch".into(),
            disclaimer: "not medical advice".into(),
        }
    }

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn trust_for(k: &SigningKey) -> TrustStore {
        let mut t = TrustStore::new();
        t.add(k.verifying_key().to_bytes());
        t
    }

    #[test]
    fn shareable_definition_signs_and_verifies() {
        let signed = SignedAgentDefinition::create(def("a"), &key(1)).unwrap();
        assert!(signed.verify_signature().is_ok());
        assert!(signed.content_id().unwrap().starts_with("sha256-"));
    }

    #[test]
    fn tampering_breaks_signature() {
        let mut signed = SignedAgentDefinition::create(def("a"), &key(1)).unwrap();
        signed.definition.name = "evil".into();
        assert!(signed.verify_signature().is_err());
    }

    #[test]
    fn untrusted_key_is_rejected_on_import() {
        let signed = SignedAgentDefinition::create(def("a"), &key(1)).unwrap();
        assert!(matches!(
            signed.verify(&TrustStore::new()).unwrap_err(),
            ShareError::Untrusted(_)
        ));
    }

    #[test]
    fn trusted_key_imports() {
        let k = key(2);
        let signed = SignedAgentDefinition::create(def("a"), &k).unwrap();
        let imported = signed.verify(&trust_for(&k)).unwrap();
        assert_eq!(imported.id, "a");
    }

    #[test]
    fn json_round_trip_preserves_verification() {
        let signed = SignedAgentDefinition::create(def("a"), &key(3)).unwrap();
        let json = signed.to_json().unwrap();
        let back = SignedAgentDefinition::from_json(&json).unwrap();
        assert!(back.verify_signature().is_ok());
        assert_eq!(back.content_id().unwrap(), signed.content_id().unwrap());
    }

    #[test]
    fn non_shareable_definition_is_refused() {
        // A genotype smuggled into the data scope must be refused at sign time.
        let mut d = def("a");
        d.data_scope.rsids = vec!["AG".into()]; // genotype, not a public id
        assert!(matches!(
            SignedAgentDefinition::create(d, &key(1)).unwrap_err(),
            ShareError::NotShareable(_)
        ));
    }

    #[test]
    fn content_id_is_stable_and_content_dependent() {
        let a = SignedAgentDefinition::create(def("x"), &key(1)).unwrap();
        let a2 = SignedAgentDefinition::create(def("x"), &key(1)).unwrap();
        let b = SignedAgentDefinition::create(def("y"), &key(1)).unwrap();
        assert_eq!(a.content_id().unwrap(), a2.content_id().unwrap());
        assert_ne!(a.content_id().unwrap(), b.content_id().unwrap());
    }
}
