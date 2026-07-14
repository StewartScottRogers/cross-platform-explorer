//! Agent-catalog trust (CPE-308 part 1 / CPE-371).
//!
//! An agent manifest declares install/run commands that execute arbitrary code with the user's
//! injected credentials — so a manifest arriving from anywhere other than the bundled first-party
//! catalog must be **signed by a trusted first-party key** before it is loaded. This module is the
//! sidecar-local verifier.
//!
//! It is deliberately format-compatible with the host's trust engine (`sidecar_host::trust`,
//! CPE-295): a detached ed25519 signature (hex) over the exact manifest bytes, checked against a
//! set of trusted public keys (hex). We re-implement it here rather than call the host because a
//! sidecar depends only on the contract (ADR 0001) — the same reason the transcript redactor is
//! sidecar-local. The trusted keys are supplied by the caller (never hardcoded here); how they are
//! distributed is a part-2 decision.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};

/// Verify a detached ed25519 `signature_hex` over `bytes` against any of `trusted_keys` (hex).
/// Returns false on any malformed input or if no trusted key matches — fail-closed, so an
/// unverifiable manifest is never trusted. Matches `sidecar_host::trust::verify_signature`.
pub fn verify_manifest(bytes: &[u8], signature_hex: &str, trusted_keys: &[String]) -> bool {
    let Ok(sig_bytes) = hex::decode(signature_hex.trim()) else { return false };
    let Ok(sig_arr): Result<[u8; 64], _> = sig_bytes.try_into() else { return false };
    let sig = Signature::from_bytes(&sig_arr);

    trusted_keys.iter().any(|pk| {
        let Ok(pk_bytes) = hex::decode(pk.trim()) else { return false };
        let Ok(pk_arr): Result<[u8; 32], _> = pk_bytes.try_into() else { return false };
        let Ok(key) = VerifyingKey::from_bytes(&pk_arr) else { return false };
        key.verify(bytes, &sig).is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn keypair(seed: u8) -> (SigningKey, String) {
        let signing = SigningKey::from_bytes(&[seed; 32]);
        (signing.clone(), hex::encode(signing.verifying_key().to_bytes()))
    }
    fn sign(k: &SigningKey, msg: &[u8]) -> String {
        hex::encode(k.sign(msg).to_bytes())
    }

    #[test]
    fn a_valid_first_party_signature_verifies() {
        let (k, pk) = keypair(1);
        let manifest = br#"{"schema_version":1,"id":"claude"}"#;
        assert!(verify_manifest(manifest, &sign(&k, manifest), &[pk]));
    }

    #[test]
    fn a_tampered_manifest_fails() {
        let (k, pk) = keypair(1);
        let sig = sign(&k, b"original");
        assert!(!verify_manifest(b"tampered", &sig, &[pk]));
    }

    #[test]
    fn a_signature_from_an_untrusted_key_fails() {
        let (k, _) = keypair(2);
        let trusted = keypair(9).1; // a DIFFERENT key
        let msg = b"x";
        assert!(!verify_manifest(msg, &sign(&k, msg), &[trusted]));
    }

    #[test]
    fn garbage_signature_or_key_is_fail_closed() {
        let (k, pk) = keypair(1);
        let msg = b"x";
        let good = sign(&k, msg);
        assert!(!verify_manifest(msg, "not-hex", &[pk.clone()]));
        assert!(!verify_manifest(msg, &good, &["not-hex".into()]));
        assert!(!verify_manifest(msg, &good, &[])); // no trusted keys → nothing trusted
    }

    #[test]
    fn any_trusted_key_matching_is_enough() {
        let (k, pk) = keypair(3);
        let msg = b"y";
        let keys = vec![keypair(4).1, pk, keypair(5).1];
        assert!(verify_manifest(msg, &sign(&k, msg), &keys));
    }
}
