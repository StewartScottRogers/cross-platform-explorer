//! Signed model-catalog snapshot (CPE-450 producer / CPE-451 client verify).
//!
//! The dynamic model catalog ([`crate::model_catalog`]) is normally fetched live from each
//! reseller. To load **fast + offline**, a scheduled job normalizes every reseller's model list
//! into one bundle, signs it with the first-party key, and publishes it to GitHub Releases; clients
//! download it, verify it, and keep the last good copy for offline use.
//!
//! This module is the **testable trust core** shared by both ends. It deliberately mirrors the
//! agent-catalog trust engine ([`crate::catalog`] / `sidecar_host::catalog` / `sidecar_host::trust`,
//! CPE-295/308/371): a detached **ed25519** signature (hex) over the snapshot's **canonical bytes**,
//! checked against a set of trusted first-party public keys (hex), plus a strictly-monotonic
//! `version` counter for **anti-rollback**. Everything here is pure — no network, no filesystem — so
//! it unit-tests headlessly. Verification is **fail-safe**: any malformed input or mismatch yields
//! `false`/rejection, so an unverifiable snapshot is never trusted.
//!
//! What is *not* here (honest scope — runtime/GUI follow-ups): the scheduled CI regeneration job
//! (CPE-450), the host-mediated allow-listed fetch, hot-reload into the running [`ResellerRegistry`],
//! and the offline/stale "as of <date>" UI (CPE-451).

use std::path::Path;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::model_catalog::{normalize_models, Model};

/// The snapshot-schema version this build understands. Bumped only on a breaking shape change.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// A signed, normalized point-in-time snapshot of the whole model catalog. `version` is the
/// monotonic anti-rollback counter (a fetched snapshot must be strictly newer than the installed
/// one to be accepted); `generated_at` is an RFC 3339 timestamp carried for the "as of <date>"
/// offline indication. The detached signature is computed over [`canonical_bytes`], never over the
/// wire JSON directly, so re-serialization (field ordering, model ordering) can't break a signature.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModelSnapshot {
    /// Monotonic catalog version — anti-rollback counter (see [`accept_snapshot`]).
    pub version: u64,
    /// RFC 3339 timestamp of when the snapshot was produced (display / staleness).
    pub generated_at: String,
    /// Every model offered across every reseller at snapshot time.
    pub models: Vec<Model>,
}

impl ModelSnapshot {
    pub fn new(version: u64, generated_at: impl Into<String>, models: Vec<Model>) -> Self {
        Self { version, generated_at: generated_at.into(), models }
    }
}

/// A **deterministic** serialization of a snapshot — the exact bytes that get signed and verified.
///
/// Models are sorted by `(reseller, id)` so producer and client agree on byte-identical content
/// regardless of the order the models arrived in; struct field order is fixed by the declaration.
/// This mirrors the agent catalog signing over its index bytes (`sidecar_host::catalog::sign_bundle`)
/// while removing any ordering ambiguity in the model list.
pub fn canonical_bytes(snapshot: &ModelSnapshot) -> Vec<u8> {
    let mut models = snapshot.models.clone();
    models.sort_by(|a, b| (a.reseller.as_str(), a.id.as_str()).cmp(&(b.reseller.as_str(), b.id.as_str())));
    let canonical = ModelSnapshot {
        version: snapshot.version,
        generated_at: snapshot.generated_at.clone(),
        models,
    };
    // Serialization of a fixed struct with sorted models is deterministic; the fallback is only
    // reachable if serde itself fails (it won't for these plain types), and an empty message simply
    // fails to verify — never a panic.
    serde_json::to_vec(&canonical).unwrap_or_default()
}

/// Hex-encoded SHA-256 of a snapshot's [`canonical_bytes`] — its stable content identity (parity
/// with `sidecar_host::trust::content_hash`). Handy for the producer to log/stamp and for a client
/// to cheaply compare "same snapshot" without re-verifying a signature.
pub fn content_hash(snapshot: &ModelSnapshot) -> String {
    let mut hasher = Sha256::new();
    hasher.update(canonical_bytes(snapshot));
    hex::encode(hasher.finalize())
}

/// Producer side (CPE-450): sign a snapshot with a 32-byte ed25519 seed (hex) and return the
/// detached signature as hex over [`canonical_bytes`]. Mirrors `sidecar_host::catalog::sign_bundle`
/// so the output verifies under [`verify_snapshot`] with the seed's public key. The key is supplied
/// by the caller (the existing `CPE_CATALOG_SIGNING_KEY`) — never hardcoded, never logged.
pub fn sign_snapshot(signing_key_hex: &str, snapshot: &ModelSnapshot) -> Result<String, String> {
    let seed = hex::decode(signing_key_hex.trim()).map_err(|e| format!("bad key hex: {e}"))?;
    let seed: [u8; 32] =
        seed.try_into().map_err(|_| "signing key must be a 32-byte seed".to_string())?;
    let key = SigningKey::from_bytes(&seed);
    Ok(hex::encode(key.sign(&canonical_bytes(snapshot)).to_bytes()))
}

/// Producer side (CPE-450): build a [`ModelSnapshot`] from a directory of raw reseller `/models`
/// responses. Each `<reseller>.json` file in `dir` is read and passed to
/// [`normalize_models`](crate::model_catalog::normalize_models) with the **file stem** as the
/// reseller id (so `openrouter.json` normalizes as `openrouter`, `github-models.json` as
/// `github-models`), and every resulting [`Model`] is collected into one snapshot.
///
/// Tolerant by design — the exact contract `list_dir` keeps: a missing directory, an unreadable
/// file, or garbage JSON is **skipped**, never fatal. A reseller whose response failed to fetch (so
/// its file is absent) simply contributes no models; one bad file never sinks the whole snapshot.
/// Files are processed in sorted order for a stable result, and [`canonical_bytes`] re-sorts the
/// models before signing so the signature is order-independent regardless.
pub fn snapshot_from_reseller_dir(dir: &Path, version: u64, generated_at: String) -> ModelSnapshot {
    let mut models = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut paths: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
            .collect();
        paths.sort();
        for path in &paths {
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            let Ok(body) = std::fs::read_to_string(path) else { continue };
            // `normalize_models` is itself total on malformed JSON (yields no models), so a garbage
            // file contributes nothing rather than erroring.
            models.extend(normalize_models(stem, &body));
        }
    }
    ModelSnapshot::new(version, generated_at, models)
}

/// Client side (CPE-451): verify a detached `signature_hex` over `snapshot`'s canonical bytes
/// against any of `trusted_keys` (hex). Returns `false` on any malformed input or if no trusted key
/// matches — **fail-closed**, so an unverifiable snapshot is never trusted. Matches
/// `crate::catalog::verify_manifest` / `sidecar_host::catalog::verify_index`.
pub fn verify_snapshot(snapshot: &ModelSnapshot, signature_hex: &str, trusted_keys: &[String]) -> bool {
    let bytes = canonical_bytes(snapshot);
    let Ok(sig_bytes) = hex::decode(signature_hex.trim()) else { return false };
    let Ok(sig_arr): Result<[u8; 64], _> = sig_bytes.try_into() else { return false };
    let sig = Signature::from_bytes(&sig_arr);

    trusted_keys.iter().any(|pk| {
        let Ok(pk_bytes) = hex::decode(pk.trim()) else { return false };
        let Ok(pk_arr): Result<[u8; 32], _> = pk_bytes.try_into() else { return false };
        let Ok(key) = VerifyingKey::from_bytes(&pk_arr) else { return false };
        key.verify(&bytes, &sig).is_ok()
    })
}

/// Anti-rollback (CPE-451): accept `incoming` only when it is strictly newer than what's installed
/// (or nothing is installed yet). Mirrors `sidecar_host::catalog::CatalogEntry::is_upgrade_over` /
/// `gate_manifest` semantics. Callers MUST have [`verify_snapshot`]'d the incoming snapshot first;
/// this enforces the monotonic-version rule only.
pub fn accept_snapshot(current_version: Option<u64>, incoming: &ModelSnapshot) -> bool {
    current_version.is_none_or(|v| incoming.version > v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_catalog::{Model, Pricing};

    fn keypair(seed: u8) -> (SigningKey, String) {
        let k = SigningKey::from_bytes(&[seed; 32]);
        (k.clone(), hex::encode(k.verifying_key().to_bytes()))
    }

    fn model(reseller: &str, id: &str) -> Model {
        Model {
            id: id.into(),
            reseller: reseller.into(),
            display_name: id.into(),
            context_length: Some(128_000),
            pricing: Pricing { prompt: Some(0.000003), completion: Some(0.000015) },
            modalities: vec!["text".into()],
            moderated: false,
        }
    }

    fn snapshot(version: u64) -> ModelSnapshot {
        ModelSnapshot {
            version,
            generated_at: "2026-07-15T00:00:00Z".into(),
            models: vec![
                model("openrouter", "anthropic/claude-3.5-sonnet"),
                model("groq", "llama-3.1-70b"),
            ],
        }
    }

    #[test]
    fn sign_then_verify_round_trips_under_the_signing_key() {
        let (k, pk) = keypair(1);
        let seed_hex = hex::encode(k.to_bytes());
        let snap = snapshot(1);
        let sig = sign_snapshot(&seed_hex, &snap).unwrap();
        assert!(verify_snapshot(&snap, &sig, std::slice::from_ref(&pk)));
        // Any trusted key in the set matching is enough.
        let keys = vec![keypair(4).1, pk, keypair(5).1];
        assert!(verify_snapshot(&snap, &sig, &keys));
    }

    #[test]
    fn canonical_bytes_are_order_independent() {
        // Two snapshots with the same models in a different order sign to the same bytes/hash.
        let a = snapshot(3);
        let mut b = a.clone();
        b.models.reverse();
        assert_eq!(canonical_bytes(&a), canonical_bytes(&b));
        assert_eq!(content_hash(&a), content_hash(&b));
        // ...and a signature made over one verifies against the other (they're byte-identical).
        let (k, pk) = keypair(2);
        let sig = sign_snapshot(&hex::encode(k.to_bytes()), &a).unwrap();
        assert!(verify_snapshot(&b, &sig, &[pk]));
    }

    #[test]
    fn verify_fails_under_a_wrong_key() {
        let (k, _) = keypair(2);
        let trusted = keypair(9).1; // a DIFFERENT key
        let snap = snapshot(1);
        let sig = sign_snapshot(&hex::encode(k.to_bytes()), &snap).unwrap();
        assert!(!verify_snapshot(&snap, &sig, &[trusted]));
        // No trusted keys at all → nothing is trusted.
        assert!(!verify_snapshot(&snap, &sig, &[]));
    }

    #[test]
    fn verify_fails_on_a_tampered_snapshot() {
        let (k, pk) = keypair(1);
        let snap = snapshot(1);
        let sig = sign_snapshot(&hex::encode(k.to_bytes()), &snap).unwrap();

        // Tamper with the model list — a model id was swapped.
        let mut tampered = snap.clone();
        tampered.models[0].id = "anthropic/claude-3-opus".into();
        assert!(!verify_snapshot(&tampered, &sig, std::slice::from_ref(&pk)));

        // Tamper with the version (a rollback dressed up with a stolen signature).
        let mut bumped = snap.clone();
        bumped.version = 999;
        assert!(!verify_snapshot(&bumped, &sig, std::slice::from_ref(&pk)));

        // Tamper with the timestamp — it's covered by the signature too.
        let mut retimed = snap.clone();
        retimed.generated_at = "1999-01-01T00:00:00Z".into();
        assert!(!verify_snapshot(&retimed, &sig, &[pk]));
    }

    #[test]
    fn verify_is_fail_closed_on_garbage_signatures_and_keys() {
        let (k, pk) = keypair(1);
        let snap = snapshot(1);
        let good = sign_snapshot(&hex::encode(k.to_bytes()), &snap).unwrap();
        assert!(!verify_snapshot(&snap, "not-hex", std::slice::from_ref(&pk)));
        assert!(!verify_snapshot(&snap, "dead", std::slice::from_ref(&pk))); // hex, wrong length
        assert!(!verify_snapshot(&snap, &good, &["not-hex".into()]));
        assert!(!verify_snapshot(&snap, &good, &["dead".into()])); // hex key, wrong length
        assert!(!verify_snapshot(&snap, "", &[pk])); // empty sig
    }

    #[test]
    fn signing_rejects_a_malformed_seed_without_panicking() {
        let snap = snapshot(1);
        assert!(sign_snapshot("not-hex", &snap).is_err());
        assert!(sign_snapshot("dead", &snap).is_err()); // hex but not 32 bytes
        assert!(sign_snapshot("", &snap).is_err());
    }

    #[test]
    fn accept_snapshot_enforces_strict_monotonic_anti_rollback() {
        // First install (nothing yet) always accepts.
        assert!(accept_snapshot(None, &snapshot(1)));
        // Strictly higher accepts.
        assert!(accept_snapshot(Some(5), &snapshot(6)));
        // Equal is rejected (a replay).
        assert!(!accept_snapshot(Some(5), &snapshot(5)));
        // Lower is rejected (a rollback).
        assert!(!accept_snapshot(Some(5), &snapshot(4)));
    }

    #[test]
    fn snapshot_from_reseller_dir_collects_models_from_every_reseller_response() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, body: &str| {
            let mut f = std::fs::File::create(dir.path().join(name)).unwrap();
            f.write_all(body.as_bytes()).unwrap();
        };

        // An OpenRouter-shaped response (`{ "data": [...] }`, string prices).
        write(
            "openrouter.json",
            r#"{"data":[
                {"id":"anthropic/claude-3.5-sonnet","name":"Claude 3.5 Sonnet","context_length":200000,
                 "pricing":{"prompt":"0.000003","completion":"0.000015"},
                 "architecture":{"input_modalities":["text","image"]}},
                {"id":"meta-llama/llama-3-8b","name":"Llama 3 8B"}
            ]}"#,
        );
        // A GitHub-Models-shaped response (a top-level array).
        write(
            "github-models.json",
            r#"[{"id":"openai/gpt-4o","name":"GPT-4o","supported_input_modalities":["text","image"]}]"#,
        );
        // A garbage file and a non-JSON file are both tolerated (skipped), not fatal.
        write("broken.json", "not json at all");
        write("notes.txt", "ignored — not a .json reseller response");

        let snap = snapshot_from_reseller_dir(dir.path(), 42, "2026-07-15T00:00:00Z".into());

        assert_eq!(snap.version, 42);
        assert_eq!(snap.generated_at, "2026-07-15T00:00:00Z");
        // Two OpenRouter models + one GitHub model + zero from the broken/txt files.
        assert_eq!(snap.models.len(), 3);
        // Models carry their source reseller (from the file stem).
        assert!(snap.models.iter().any(|m| m.reseller == "openrouter" && m.id == "anthropic/claude-3.5-sonnet"));
        assert!(snap.models.iter().any(|m| m.reseller == "openrouter" && m.id == "meta-llama/llama-3-8b"));
        assert!(snap.models.iter().any(|m| m.reseller == "github-models" && m.id == "openai/gpt-4o"));

        // The whole bundle signs + verifies as one snapshot.
        let (k, pk) = keypair(6);
        let sig = sign_snapshot(&hex::encode(k.to_bytes()), &snap).unwrap();
        assert!(verify_snapshot(&snap, &sig, &[pk]));
    }

    #[test]
    fn snapshot_from_reseller_dir_tolerates_a_missing_directory() {
        let missing = std::path::Path::new("this-directory-does-not-exist-cpe450");
        let snap = snapshot_from_reseller_dir(missing, 1, "2026-07-15T00:00:00Z".into());
        assert!(snap.models.is_empty());
        assert_eq!(snap.version, 1);
    }

    #[test]
    fn snapshot_json_round_trips_and_malformed_input_never_panics() {
        let snap = snapshot(7);
        let json = serde_json::to_string(&snap).unwrap();
        let back: ModelSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snap, back);
        // Malformed JSON is an Err, not a panic.
        assert!(serde_json::from_str::<ModelSnapshot>("not json").is_err());
        assert!(serde_json::from_str::<ModelSnapshot>("{}").is_err());
        // An empty-model snapshot still signs + verifies (no special-casing).
        let (k, pk) = keypair(3);
        let empty = ModelSnapshot { version: 1, generated_at: "2026-07-15T00:00:00Z".into(), models: vec![] };
        let sig = sign_snapshot(&hex::encode(k.to_bytes()), &empty).unwrap();
        assert!(verify_snapshot(&empty, &sig, &[pk]));
    }
}
