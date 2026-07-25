//! Manifest trust, provenance & signing (CPE-295) — the program's most important
//! security surface.
//!
//! A sidecar or agent manifest declares install/run commands that **execute arbitrary
//! code** with the user's injected credentials. A tampered or malicious manifest is
//! remote code execution. So:
//!
//! - **First-party** manifests are **signed** (ed25519); a valid signature from a
//!   trusted key means [`TrustDecision::TrustedSigned`].
//! - Everything else is **untrusted** until the user consents, keyed by the manifest's
//!   content hash. Any change to the content (new hash) re-prompts.
//!
//! This module is the trust *engine* (verification + consent state + provenance). The
//! host UI presents the disclosure ("this will run …") and captures the consent.

use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

/// The trust status of a manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustDecision {
    /// Carries a valid signature from a trusted (first-party) key.
    TrustedSigned,
    /// Unsigned/third-party, but the user has consented to this exact content.
    Consented,
    /// Unsigned/third-party and not consented — must be disclosed + consented before
    /// any of its commands run.
    Untrusted,
}

/// Hex-encoded SHA-256 of `bytes` — the stable identity of a manifest's content.
pub fn content_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Verify an ed25519 signature (hex) over `msg` against a public key (hex).
pub fn verify_signature(msg: &[u8], signature_hex: &str, pubkey_hex: &str) -> bool {
    let Ok(pk_bytes) = hex::decode(pubkey_hex) else { return false };
    let Ok(pk_arr): Result<[u8; 32], _> = pk_bytes.try_into() else { return false };
    let Ok(key) = VerifyingKey::from_bytes(&pk_arr) else { return false };

    let Ok(sig_bytes) = hex::decode(signature_hex) else { return false };
    let Ok(sig_arr): Result<[u8; 64], _> = sig_bytes.try_into() else { return false };
    let sig = Signature::from_bytes(&sig_arr);

    key.verify(msg, &sig).is_ok()
}

/// Where a manifest came from, for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    pub source: String,
    pub hash: String,
    pub first_seen: String,
}

/// The trust engine: the set of trusted first-party public keys, and the set of
/// content hashes the user has consented to. Optionally a policy allowlist of hashes
/// (locked-down/enterprise deployments) — when set, only listed hashes may be trusted
/// by consent.
#[derive(Debug, Clone, Default)]
pub struct TrustStore {
    pub trusted_keys: Vec<String>,
    consented: BTreeSet<String>,
    provenance: BTreeMap<String, Provenance>,
    pub policy_allow: Option<BTreeSet<String>>,
}

impl TrustStore {
    pub fn new(trusted_keys: Vec<String>) -> Self {
        Self { trusted_keys, ..Default::default() }
    }

    /// Decide how much to trust `bytes`, optionally accompanied by a `signature`.
    pub fn evaluate(&self, bytes: &[u8], signature_hex: Option<&str>) -> TrustDecision {
        if let Some(sig) = signature_hex {
            if self.trusted_keys.iter().any(|pk| verify_signature(bytes, sig, pk)) {
                return TrustDecision::TrustedSigned;
            }
        }
        let hash = content_hash(bytes);
        let policy_ok = self.policy_allow.as_ref().is_none_or(|a| a.contains(&hash));
        if policy_ok && self.consented.contains(&hash) {
            TrustDecision::Consented
        } else {
            TrustDecision::Untrusted
        }
    }

    /// Record the user's consent to this exact content (after disclosure). Consent is
    /// bound to the content hash, so any later change re-prompts.
    pub fn record_consent(&mut self, bytes: &[u8]) {
        self.consented.insert(content_hash(bytes));
    }

    /// Record where a manifest came from (for the provenance UI).
    pub fn record_provenance(&mut self, bytes: &[u8], source: impl Into<String>, first_seen: impl Into<String>) {
        let hash = content_hash(bytes);
        self.provenance
            .entry(hash.clone())
            .or_insert(Provenance { source: source.into(), hash, first_seen: first_seen.into() });
    }

    pub fn provenance_of(&self, bytes: &[u8]) -> Option<&Provenance> {
        self.provenance.get(&content_hash(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn keypair(seed: u8) -> (SigningKey, String) {
        let signing = SigningKey::from_bytes(&[seed; 32]);
        let pub_hex = hex::encode(signing.verifying_key().to_bytes());
        (signing, pub_hex)
    }

    fn sign(signing: &SigningKey, msg: &[u8]) -> String {
        hex::encode(signing.sign(msg).to_bytes())
    }

    #[test]
    fn a_valid_first_party_signature_is_trusted() {
        let (signing, pub_hex) = keypair(1);
        let store = TrustStore::new(vec![pub_hex]);
        let manifest = b"{ id: claude, install: npm ... }";
        let sig = sign(&signing, manifest);
        assert_eq!(store.evaluate(manifest, Some(&sig)), TrustDecision::TrustedSigned);
    }

    #[test]
    fn a_tampered_manifest_fails_verification() {
        let (signing, pub_hex) = keypair(1);
        let store = TrustStore::new(vec![pub_hex]);
        let sig = sign(&signing, b"original");
        // Same signature, different bytes → not trusted.
        assert_eq!(store.evaluate(b"tampered", Some(&sig)), TrustDecision::Untrusted);
    }

    #[test]
    fn a_signature_from_an_untrusted_key_is_not_trusted() {
        let (signing, _pub_hex) = keypair(2);
        let store = TrustStore::new(vec![keypair(9).1]); // trusts a DIFFERENT key
        let msg = b"x";
        let sig = sign(&signing, msg);
        assert_eq!(store.evaluate(msg, Some(&sig)), TrustDecision::Untrusted);
    }

    #[test]
    fn unsigned_requires_consent_then_is_consented_until_changed() {
        let mut store = TrustStore::default();
        let manifest = b"third-party manifest v1";
        assert_eq!(store.evaluate(manifest, None), TrustDecision::Untrusted);
        store.record_consent(manifest);
        assert_eq!(store.evaluate(manifest, None), TrustDecision::Consented);
        // A change re-prompts.
        assert_eq!(store.evaluate(b"third-party manifest v2", None), TrustDecision::Untrusted);
    }

    #[test]
    fn policy_allowlist_gates_consent() {
        let mut store = TrustStore::default();
        let manifest = b"m";
        store.record_consent(manifest);
        // Consent alone would pass, but a policy that doesn't list the hash blocks it.
        store.policy_allow = Some(BTreeSet::new());
        assert_eq!(store.evaluate(manifest, None), TrustDecision::Untrusted);
        // Allow the hash → consented again.
        store.policy_allow = Some(BTreeSet::from([content_hash(manifest)]));
        assert_eq!(store.evaluate(manifest, None), TrustDecision::Consented);
    }

    #[test]
    fn provenance_is_recorded_once_by_hash() {
        let mut store = TrustStore::default();
        let m = b"m";
        store.record_provenance(m, "https://example.com", "2026-07-13");
        assert_eq!(store.provenance_of(m).unwrap().source, "https://example.com");
    }
}
