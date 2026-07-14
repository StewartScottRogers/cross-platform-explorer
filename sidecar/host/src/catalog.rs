//! Signed agent-catalog index (CPE-308 part 2, slice 1).
//!
//! Host-authoritative trust for runtime catalog updates (design decision D1). A catalog **index**
//! is a signed document that lists agent-manifest entries; the host verifies the index against a
//! trusted first-party key (reusing the CPE-295 trust engine), binds each entry to its manifest
//! **content** by SHA-256, and enforces **monotonic versions** (anti-rollback) before any manifest
//! is handed to the sidecar for loading. The sidecar's own signature check (CPE-371) then remains
//! as defence-in-depth.
//!
//! The index signature is detached — over the exact index bytes — mirroring the per-manifest `.sig`
//! convention from CPE-371, so there is no JSON-canonicalisation ambiguity.

use serde::{Deserialize, Serialize};

use crate::trust;

/// The index-schema version this build understands.
pub const CATALOG_SCHEMA_VERSION: u16 = 1;

/// One agent manifest named by the index. `sha256` binds the entry to exact manifest content;
/// `version` is a monotonic counter used for anti-rollback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    /// The manifest's own agent-schema version (CPE-278/300), carried for migration decisions.
    pub schema_version: u16,
    /// Hex SHA-256 of the manifest bytes this entry names.
    pub sha256: String,
    /// Monotonic catalog version for this entry — a fetched entry must be strictly newer than the
    /// installed one to be accepted (anti-rollback).
    pub version: u64,
}

/// A signed list of catalog entries. Verified as a whole via a detached signature over its bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CatalogIndex {
    pub schema_version: u16,
    #[serde(default)]
    pub entries: Vec<CatalogEntry>,
}

impl CatalogIndex {
    pub fn from_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| e.to_string())
    }
    pub fn get(&self, id: &str) -> Option<&CatalogEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
    /// Whether this build understands the index schema (unknown-future is refused, as elsewhere).
    pub fn is_supported(&self) -> bool {
        self.schema_version != 0 && self.schema_version <= CATALOG_SCHEMA_VERSION
    }
}

impl CatalogEntry {
    /// Whether `manifest_bytes` is exactly the content this entry names (content binding).
    pub fn matches(&self, manifest_bytes: &[u8]) -> bool {
        trust::content_hash(manifest_bytes).eq_ignore_ascii_case(self.sha256.trim())
    }
    /// Anti-rollback: accept only a strictly newer version than what's installed (or a first install).
    pub fn is_upgrade_over(&self, installed: Option<u64>) -> bool {
        installed.is_none_or(|v| self.version > v)
    }
}

/// Verify a detached signature over the index bytes against any trusted key (CPE-295 format).
/// Fail-closed: false on any malformed input or if no trusted key matches.
pub fn verify_index(index_bytes: &[u8], signature_hex: &str, trusted_keys: &[String]) -> bool {
    trusted_keys.iter().any(|pk| trust::verify_signature(index_bytes, signature_hex, pk))
}

/// The result of gating one incoming manifest against a (signature-verified) index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryVerdict {
    /// Content matches the index and it is a strict upgrade — safe to load.
    Accept,
    /// The id is not listed in the index.
    Unlisted,
    /// Listed, but the content hash does not match (tamper).
    ContentMismatch,
    /// Listed, but not newer than what is installed (rollback attempt).
    Rollback,
}

/// Gate one manifest by id + content against an **already signature-verified** index and the
/// installed version. Callers MUST call [`verify_index`] first; this enforces content binding and
/// anti-rollback only.
pub fn gate_manifest(
    index: &CatalogIndex,
    id: &str,
    manifest_bytes: &[u8],
    installed_version: Option<u64>,
) -> EntryVerdict {
    let Some(entry) = index.get(id) else {
        return EntryVerdict::Unlisted;
    };
    if !entry.matches(manifest_bytes) {
        return EntryVerdict::ContentMismatch;
    }
    if !entry.is_upgrade_over(installed_version) {
        return EntryVerdict::Rollback;
    }
    EntryVerdict::Accept
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn keypair(seed: u8) -> (SigningKey, String) {
        let k = SigningKey::from_bytes(&[seed; 32]);
        (k.clone(), hex::encode(k.verifying_key().to_bytes()))
    }
    fn sign(k: &SigningKey, msg: &[u8]) -> String {
        hex::encode(k.sign(msg).to_bytes())
    }
    fn index_json(id: &str, sha: &str, version: u64) -> String {
        format!(
            r#"{{"schema_version":1,"entries":[{{"id":"{id}","schema_version":1,"sha256":"{sha}","version":{version}}}]}}"#
        )
    }

    #[test]
    fn a_valid_index_signature_verifies_and_a_tampered_one_does_not() {
        let (k, pk) = keypair(1);
        let bytes = index_json("claude", "deadbeef", 3);
        let sig = sign(&k, bytes.as_bytes());
        assert!(verify_index(bytes.as_bytes(), &sig, &[pk.clone()]));
        assert!(!verify_index(b"tampered", &sig, &[pk]));
        assert!(!verify_index(bytes.as_bytes(), &sig, &[keypair(9).1])); // untrusted key
    }

    #[test]
    fn gate_accepts_matching_content_that_is_an_upgrade() {
        let manifest = br#"{"schema_version":1,"id":"claude"}"#;
        let sha = trust::content_hash(manifest);
        let index = CatalogIndex::from_json(&index_json("claude", &sha, 5)).unwrap();
        assert_eq!(gate_manifest(&index, "claude", manifest, Some(4)), EntryVerdict::Accept);
        assert_eq!(gate_manifest(&index, "claude", manifest, None), EntryVerdict::Accept);
    }

    #[test]
    fn gate_rejects_unlisted_tampered_and_rollback() {
        let manifest = br#"{"schema_version":1,"id":"claude"}"#;
        let sha = trust::content_hash(manifest);
        let index = CatalogIndex::from_json(&index_json("claude", &sha, 5)).unwrap();
        // Unknown id.
        assert_eq!(gate_manifest(&index, "aider", manifest, None), EntryVerdict::Unlisted);
        // Right id, wrong content.
        assert_eq!(
            gate_manifest(&index, "claude", b"different bytes", None),
            EntryVerdict::ContentMismatch
        );
        // Same version as installed → not an upgrade → rollback attempt.
        assert_eq!(gate_manifest(&index, "claude", manifest, Some(5)), EntryVerdict::Rollback);
        assert_eq!(gate_manifest(&index, "claude", manifest, Some(6)), EntryVerdict::Rollback);
    }

    #[test]
    fn unsupported_index_schema_is_flagged() {
        let idx = CatalogIndex { schema_version: 99, entries: vec![] };
        assert!(!idx.is_supported());
        assert!(CatalogIndex { schema_version: 1, entries: vec![] }.is_supported());
    }
}
