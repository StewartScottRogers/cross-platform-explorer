//! Headless verification of the Tauri updater manifest (`latest.json`) and its minisign signatures.
//!
//! The shipped app auto-updates via `tauri_plugin_updater`: at runtime the plugin fetches `latest.json`,
//! compares versions, downloads the artifact, and verifies the artifact's **minisign signature** against
//! the `pubkey` in `tauri.conf.json` before swapping the binary. Most of what a human checks after a
//! release is *artifact correctness*, not GUI behaviour — a malformed manifest, a signature that won't
//! verify against the configured pubkey, a version mismatch, a missing URL. All of that is checkable
//! without a GUI or a running app, which is what this crate does (CPE-1058, manual-test burndown #6).
//!
//! # Minisign format (the load-bearing detail)
//!
//! Tauri **double-base64-encodes** both the config `pubkey` and each manifest `signature`:
//!
//! * base64-decoding the config `pubkey` yields a minisign **public-key file** (two lines: an
//!   `untrusted comment:` line then the `RW…` key line) — parsed here via
//!   [`minisign::PublicKeyBox::from_string`].
//! * base64-decoding a `signature` field yields a minisign **`.sig` file** — parsed here via
//!   [`minisign::SignatureBox::from_string`].
//!
//! The signature is then verified over the **raw artifact bytes** with [`minisign::verify`]. We never
//! hand-roll ed25519 — we drive the same library Tauri uses.

use base64::Engine as _;

/// Standard base64 (with padding) — the alphabet Tauri uses for both the config pubkey and signatures.
const B64: base64::engine::general_purpose::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// A single reason a manifest failed verification. A run collects *all* problems it finds rather than
/// stopping at the first, so a release-guard run reports everything wrong in one shot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestProblem {
    /// The manifest JSON did not parse at all.
    Unparseable(String),
    /// Top-level `version` field missing or not a string.
    MissingVersion,
    /// `version` is present but does not equal the expected (configured) version.
    VersionMismatch { expected: String, found: String },
    /// Neither a `platforms` object-map nor a `platforms` array was found.
    NoPlatforms,
    /// A platform entry has no `url`.
    MissingUrl { platform: String },
    /// A platform entry has no `signature`.
    MissingSignature { platform: String },
    /// The manifest names a platform whose artifact bytes could not be obtained locally to check its
    /// signature against. CPE-1872 (security-audit finding 1): a manifest platform that cannot be
    /// cryptographically checked is a **failure**, never a silent skip -- the old "skip what I don't
    /// have" behavior is exactly what let a smuggled platform entry (pointing at an artifact that
    /// simply never gets built/served locally) pass with `EXIT=0` while carrying an unverified,
    /// attacker-controlled URL + signature.
    ArtifactUnavailable { platform: String },
    /// The configured pubkey could not be base64-decoded or parsed as a minisign public key.
    BadPubkey(String),
    /// A platform's `signature` could not be base64-decoded or parsed as a minisign signature.
    BadSignature { platform: String, detail: String },
    /// The minisign signature did not verify against the pubkey over the artifact bytes.
    VerificationFailed { platform: String },
}

impl std::fmt::Display for ManifestProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestProblem::Unparseable(e) => write!(f, "manifest is not valid JSON: {e}"),
            ManifestProblem::MissingVersion => {
                write!(f, "manifest has no top-level string `version` field")
            }
            ManifestProblem::VersionMismatch { expected, found } => write!(
                f,
                "manifest version `{found}` does not match expected `{expected}`"
            ),
            ManifestProblem::NoPlatforms => {
                write!(f, "manifest has no `platforms` (expected an object map or array)")
            }
            ManifestProblem::MissingUrl { platform } => {
                write!(f, "platform `{platform}` has no `url`")
            }
            ManifestProblem::MissingSignature { platform } => {
                write!(f, "platform `{platform}` has no `signature`")
            }
            ManifestProblem::ArtifactUnavailable { platform } => write!(
                f,
                "platform `{platform}` has no locally-available artifact bytes to verify its signature                  against -- a manifest platform that cannot be verified is a failure, not a skip"
            ),
            ManifestProblem::BadPubkey(detail) => {
                write!(f, "configured pubkey is unusable: {detail}")
            }
            ManifestProblem::BadSignature { platform, detail } => {
                write!(f, "platform `{platform}` signature is unusable: {detail}")
            }
            ManifestProblem::VerificationFailed { platform } => write!(
                f,
                "platform `{platform}` signature did NOT verify against the configured pubkey"
            ),
        }
    }
}

impl std::error::Error for ManifestProblem {}

/// Verify a Tauri updater manifest end-to-end, mirroring the runtime plugin's checks.
///
/// Asserts, collecting every problem it finds:
/// 1. `manifest_json` parses and has the required shape (top-level `version`; per-platform `url` +
///    `signature`, in either the `platforms` object-map or array form tauri-action can emit);
/// 2. `version == expected_version`;
/// 3. **every** platform's minisign `signature` verifies against `pubkey_config_b64` over the artifact
///    bytes -- `Ok(())` means every platform the manifest names was actually, cryptographically checked.
///
/// `pubkey_config_b64` is the value straight out of `tauri.conf.json` (double-base64, as Tauri stores it).
///
/// `artifact` maps a platform's `url` to its raw bytes. Returning `None` means "I don't have this
/// artifact", and — as of CPE-1872's security-audit round — that is now a hard [`ManifestProblem::ArtifactUnavailable`]
/// failure, **not** a skip. It used to be a skip, on the theory that a per-runner release guard could
/// verify only the platforms it actually built locally while other runners verified theirs; a real-world
/// manifest is the UNION of an entire release matrix (tauri-action's `upload-version-json.ts` downloads
/// the existing published manifest and merges each runner's platform into it), so no single runner's
/// local build output ever contains every platform the manifest names — meaning "skip what's missing"
/// was, in practice, "skip whatever I didn't happen to build," which a manifest platform pointed at an
/// attacker-controlled URL sails straight through. Callers that want the old "verify only what I can
/// reach" behavior must supply `artifact` closures that can actually FETCH every platform the manifest
/// names (e.g. downloading each one from the published release, as `verify-release-artifacts` now does)
/// rather than relying on this function to look the other way. See [`manifest_platform_count`] for
/// callers that want to report "verified N of M platforms" using the manifest's own platform count.
///
/// Returns `Ok(())` only when every named platform was fetched AND verified; else `Err` with every
/// [`ManifestProblem`] found (which platform(s) were unreachable is distinguishable from which failed
/// crypto via [`ManifestProblem::ArtifactUnavailable`] vs [`ManifestProblem::VerificationFailed`]).
pub fn verify_update_manifest(
    manifest_json: &str,
    pubkey_config_b64: &str,
    expected_version: &str,
    artifact: impl Fn(&str) -> Option<Vec<u8>>,
) -> Result<(), Vec<ManifestProblem>> {
    let mut problems = Vec::new();

    // Parse the configured pubkey once. If it's unusable nothing can verify, but we still report shape
    // problems below (a bad pubkey and a malformed manifest are independent failures worth surfacing
    // together), so this is not an early return.
    let pubkey = match decode_config_pubkey(pubkey_config_b64) {
        Ok(pk) => Some(pk),
        Err(detail) => {
            problems.push(ManifestProblem::BadPubkey(detail));
            None
        }
    };

    // Parse the manifest. Nothing else is checkable if the JSON itself is broken.
    let value: serde_json::Value = match serde_json::from_str(manifest_json) {
        Ok(v) => v,
        Err(e) => {
            problems.push(ManifestProblem::Unparseable(e.to_string()));
            return Err(problems);
        }
    };

    match value.get("version").and_then(|v| v.as_str()) {
        None => problems.push(ManifestProblem::MissingVersion),
        Some(v) if v != expected_version => problems.push(ManifestProblem::VersionMismatch {
            expected: expected_version.to_string(),
            found: v.to_string(),
        }),
        Some(_) => {}
    }

    let platforms = collect_platforms(&value);
    if platforms.is_empty() {
        problems.push(ManifestProblem::NoPlatforms);
    }

    for (name, entry) in platforms {
        let url = entry.get("url").and_then(|u| u.as_str());
        let sig = entry.get("signature").and_then(|s| s.as_str());

        let url = match url {
            Some(u) => u,
            None => {
                problems.push(ManifestProblem::MissingUrl { platform: name });
                continue;
            }
        };
        let sig = match sig {
            Some(s) => s,
            None => {
                problems.push(ManifestProblem::MissingSignature { platform: name });
                continue;
            }
        };

        // Decode the signature (shape check — reported even if we can't fetch the artifact/pubkey).
        let sig_box = match decode_signature(sig) {
            Ok(s) => s,
            Err(detail) => {
                problems.push(ManifestProblem::BadSignature { platform: name, detail });
                continue;
            }
        };

        // Only the cryptographic verification below needs a usable pubkey. A bad pubkey already got
        // reported once above and nothing can verify against it, so skip crypto for every platform in
        // that case rather than repeating the same BadPubkey-adjacent noise per platform.
        let Some(pk) = pubkey.as_ref() else { continue };

        // CPE-1872 (security-audit finding 1): a platform whose artifact bytes could not be obtained is
        // a FAILURE now, never a silent skip. The manifest's platform list is the union tauri-action
        // builds across an entire release matrix, and a caller that only has some of those artifacts
        // locally must fetch (or refuse to trust) the rest -- it must never let an unverifiable platform
        // ride through as if it had been checked. `None` here used to mean "I don't have this one, skip
        // it"; it now means "this platform cannot be trusted as published."
        let bytes = match artifact(url) {
            Some(b) => b,
            None => {
                problems.push(ManifestProblem::ArtifactUnavailable { platform: name });
                continue;
            }
        };

        // quiet=true (no stderr chatter), output=false (don't echo the artifact), allow_legacy=true so
        // both minisign signature forms (prehashed + legacy ed25519) verify — Tauri uses prehashed.
        if minisign::verify(pk, &sig_box, std::io::Cursor::new(&bytes), true, false, true).is_err() {
            problems.push(ManifestProblem::VerificationFailed { platform: name });
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// Base64-decode the config `pubkey` into its inner minisign public-key file, then parse it.
fn decode_config_pubkey(pubkey_config_b64: &str) -> Result<minisign::PublicKey, String> {
    let file_bytes = B64
        .decode(pubkey_config_b64.trim())
        .map_err(|e| format!("outer base64 decode failed: {e}"))?;
    let file_text =
        String::from_utf8(file_bytes).map_err(|e| format!("decoded pubkey is not UTF-8: {e}"))?;
    minisign::PublicKeyBox::from_string(&file_text)
        .map_err(|e| format!("not a minisign public-key file: {e}"))?
        .into_public_key()
        .map_err(|e| format!("invalid minisign public key: {e}"))
}

/// Base64-decode a manifest `signature` into its inner minisign `.sig` file, then parse it.
fn decode_signature(signature_b64: &str) -> Result<minisign::SignatureBox, String> {
    let file_bytes = B64
        .decode(signature_b64.trim())
        .map_err(|e| format!("outer base64 decode failed: {e}"))?;
    let file_text =
        String::from_utf8(file_bytes).map_err(|e| format!("decoded signature is not UTF-8: {e}"))?;
    minisign::SignatureBox::from_string(&file_text)
        .map_err(|e| format!("not a minisign signature file: {e}"))
}

/// Extract the per-platform entries, handling both shapes tauri-action can emit:
/// the `platforms` object-map (`{ "windows-x86_64": { url, signature }, … }`, the common v2 form) and an
/// array of entries that each carry their own `platform`/`target` name (defensive — some tooling emits it).
fn collect_platforms(value: &serde_json::Value) -> Vec<(String, &serde_json::Value)> {
    if let Some(map) = value.get("platforms").and_then(|p| p.as_object()) {
        return map.iter().map(|(k, v)| (k.clone(), v)).collect();
    }
    if let Some(arr) = value.get("platforms").and_then(|p| p.as_array()) {
        return arr
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let name = v
                    .get("platform")
                    .or_else(|| v.get("target"))
                    .and_then(|n| n.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("[{i}]"));
                (name, v)
            })
            .collect();
    }
    Vec::new()
}

/// The number of per-platform entries a manifest names (regardless of whether it verifies), for callers
/// that want to report "verified N of M platforms" (CPE-1872) using the manifest's own shape as M rather
/// than recomputing it independently. Returns `None` if `manifest_json` does not even parse as JSON.
pub fn manifest_platform_count(manifest_json: &str) -> Option<usize> {
    let value: serde_json::Value = serde_json::from_str(manifest_json).ok()?;
    Some(collect_platforms(&value).len())
}

/// CPE-1872 round 3 (security-audit finding B): the basename-matching download/verify pipeline reduces
/// every platform `url` to its LAST path segment to find a local file to check bytes against -- which
/// means the host and every path segment *before* the basename (the repo, and the release tag) are
/// never checked against anything. A manifest can carry a perfectly genuine, correctly-signed artifact
/// under a `url` that points at `https://evil.example/...same-basename` or at the right repo but the
/// WRONG release tag, and this pipeline verifies it clean -- the bytes are real, but every real updater
/// client that trusts this manifest is pointed at infrastructure the release process never checked
/// (demonstrated: `n1_foreign_host_same_basename`, `n2_wrong_tag_same_basename`, both `EXIT=0` before
/// this check). This closes the binding side of that gap: every platform's `url` must start with
/// `expected_prefix` (the real `https://github.com/{repo}/releases/download/{tag}/` this release
/// actually publishes under), or its platform name is returned as an offender. An unparseable manifest
/// returns no offenders -- that failure is already reported elsewhere (shape errors, not a binding
/// check); a platform with a missing/non-string `url` is reported as an offender with an empty url,
/// which trivially fails the prefix check like any other malformed entry.
pub fn platforms_with_url_outside_prefix(manifest_json: &str, expected_prefix: &str) -> Vec<(String, String)> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(manifest_json) else {
        return Vec::new();
    };
    collect_platforms(&value)
        .into_iter()
        .filter_map(|(name, entry)| {
            let url = entry.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string();
            if url.starts_with(expected_prefix) {
                None
            } else {
                Some((name, url))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use minisign::{KeyPair, SignatureBox};

    /// The artifact bytes every fixture signs over.
    const ARTIFACT: &[u8] = b"cross-platform-explorer installer bytes v0.57.33";
    const VERSION: &str = "0.57.33";
    const URL: &str = "https://example.com/releases/download/v0.57.33/app_x64-setup.nsis.zip";

    /// A freshly generated keypair, encoded into the SAME double-base64 shape the real config uses, so the
    /// test exercises the real decode path (`config pubkey → base64 → minisign public-key file`).
    struct Fixture {
        pubkey_config_b64: String,
        keypair: KeyPair,
    }

    fn new_fixture() -> Fixture {
        let keypair = KeyPair::generate_unencrypted_keypair().expect("generate keypair");
        let pk_file = keypair.pk.to_box().expect("public key box").into_string();
        let pubkey_config_b64 = B64.encode(pk_file.as_bytes());
        Fixture { pubkey_config_b64, keypair }
    }

    /// Sign `data` with `sk` and return the manifest `signature` field value (double-base64), mirroring
    /// exactly how Tauri stores signatures: base64 of the minisign `.sig` file text.
    fn sign_to_field(keypair: &KeyPair, data: &[u8]) -> String {
        let sig_box: SignatureBox = minisign::sign(
            Some(&keypair.pk),
            &keypair.sk,
            std::io::Cursor::new(data),
            Some("trusted comment"),
            Some("signature from test key"),
        )
        .expect("sign");
        B64.encode(sig_box.into_string().as_bytes())
    }

    fn manifest(version: &str, url: &str, signature: &str) -> String {
        serde_json::json!({
            "version": version,
            "notes": "test release",
            "pub_date": "2026-07-25T00:00:00Z",
            "platforms": {
                "windows-x86_64": { "signature": signature, "url": url }
            }
        })
        .to_string()
    }

    fn always(bytes: &'static [u8]) -> impl Fn(&str) -> Option<Vec<u8>> {
        move |_url| Some(bytes.to_vec())
    }

    #[test]
    fn valid_manifest_verifies_ok() {
        let fx = new_fixture();
        let sig = sign_to_field(&fx.keypair, ARTIFACT);
        let m = manifest(VERSION, URL, &sig);
        let res = verify_update_manifest(&m, &fx.pubkey_config_b64, VERSION, always(ARTIFACT));
        assert!(res.is_ok(), "expected Ok, got {res:?}");
    }

    #[test]
    fn tampered_artifact_is_rejected() {
        let fx = new_fixture();
        let sig = sign_to_field(&fx.keypair, ARTIFACT);
        let m = manifest(VERSION, URL, &sig);
        // Same signature, but the artifact bytes served differ from what was signed.
        let tampered: &'static [u8] = b"this is NOT the artifact that was signed!!!!!!!!";
        let res = verify_update_manifest(&m, &fx.pubkey_config_b64, VERSION, always(tampered));
        assert_eq!(
            res.unwrap_err(),
            vec![ManifestProblem::VerificationFailed { platform: "windows-x86_64".into() }]
        );
    }

    #[test]
    fn wrong_key_is_rejected() {
        // Sign with one key, verify against a DIFFERENT config pubkey.
        let signer = new_fixture();
        let other = new_fixture();
        let sig = sign_to_field(&signer.keypair, ARTIFACT);
        let m = manifest(VERSION, URL, &sig);
        let res = verify_update_manifest(&m, &other.pubkey_config_b64, VERSION, always(ARTIFACT));
        assert_eq!(
            res.unwrap_err(),
            vec![ManifestProblem::VerificationFailed { platform: "windows-x86_64".into() }]
        );
    }

    #[test]
    fn version_mismatch_is_rejected() {
        let fx = new_fixture();
        let sig = sign_to_field(&fx.keypair, ARTIFACT);
        let m = manifest("9.9.9", URL, &sig); // manifest says 9.9.9, we expect 0.57.33
        let err = verify_update_manifest(&m, &fx.pubkey_config_b64, VERSION, always(ARTIFACT))
            .unwrap_err();
        assert!(err.contains(&ManifestProblem::VersionMismatch {
            expected: VERSION.into(),
            found: "9.9.9".into(),
        }));
    }

    #[test]
    fn unparseable_manifest_is_rejected() {
        let fx = new_fixture();
        let err = verify_update_manifest("{ not json ", &fx.pubkey_config_b64, VERSION, always(ARTIFACT))
            .unwrap_err();
        assert!(matches!(err.as_slice(), [ManifestProblem::Unparseable(_)]));
    }

    #[test]
    fn missing_version_is_rejected() {
        let fx = new_fixture();
        let sig = sign_to_field(&fx.keypair, ARTIFACT);
        let m = serde_json::json!({
            "platforms": { "windows-x86_64": { "signature": sig, "url": URL } }
        })
        .to_string();
        let err = verify_update_manifest(&m, &fx.pubkey_config_b64, VERSION, always(ARTIFACT))
            .unwrap_err();
        assert!(err.contains(&ManifestProblem::MissingVersion));
    }

    #[test]
    fn missing_signature_field_is_rejected() {
        let fx = new_fixture();
        let m = serde_json::json!({
            "version": VERSION,
            "platforms": { "windows-x86_64": { "url": URL } }
        })
        .to_string();
        let err = verify_update_manifest(&m, &fx.pubkey_config_b64, VERSION, always(ARTIFACT))
            .unwrap_err();
        assert!(err.contains(&ManifestProblem::MissingSignature { platform: "windows-x86_64".into() }));
    }

    #[test]
    fn missing_url_field_is_rejected() {
        let fx = new_fixture();
        let sig = sign_to_field(&fx.keypair, ARTIFACT);
        let m = serde_json::json!({
            "version": VERSION,
            "platforms": { "windows-x86_64": { "signature": sig } }
        })
        .to_string();
        let err = verify_update_manifest(&m, &fx.pubkey_config_b64, VERSION, always(ARTIFACT))
            .unwrap_err();
        assert!(err.contains(&ManifestProblem::MissingUrl { platform: "windows-x86_64".into() }));
    }

    #[test]
    fn no_platforms_is_rejected() {
        let fx = new_fixture();
        let m = serde_json::json!({ "version": VERSION }).to_string();
        let err = verify_update_manifest(&m, &fx.pubkey_config_b64, VERSION, always(ARTIFACT))
            .unwrap_err();
        assert!(err.contains(&ManifestProblem::NoPlatforms));
    }

    #[test]
    fn array_form_platforms_is_supported() {
        let fx = new_fixture();
        let sig = sign_to_field(&fx.keypair, ARTIFACT);
        let m = serde_json::json!({
            "version": VERSION,
            "platforms": [ { "platform": "windows-x86_64", "signature": sig, "url": URL } ]
        })
        .to_string();
        let res = verify_update_manifest(&m, &fx.pubkey_config_b64, VERSION, always(ARTIFACT));
        assert!(res.is_ok(), "array form should verify, got {res:?}");
    }

    #[test]
    fn bad_pubkey_is_reported() {
        let fx = new_fixture();
        let sig = sign_to_field(&fx.keypair, ARTIFACT);
        let m = manifest(VERSION, URL, &sig);
        let err = verify_update_manifest(&m, "!!!not base64!!!", VERSION, always(ARTIFACT))
            .unwrap_err();
        assert!(err.iter().any(|p| matches!(p, ManifestProblem::BadPubkey(_))));
    }

    #[test]
    fn bad_signature_encoding_is_reported() {
        let fx = new_fixture();
        let m = manifest(VERSION, URL, "%%%not-base64%%%");
        let err = verify_update_manifest(&m, &fx.pubkey_config_b64, VERSION, always(ARTIFACT))
            .unwrap_err();
        assert!(err
            .iter()
            .any(|p| matches!(p, ManifestProblem::BadSignature { platform, .. } if platform == "windows-x86_64")));
    }

    #[test]
    fn missing_artifact_fails_verification() {
        // CPE-1872 (security-audit finding 1): loader returns None -> that platform is now a hard
        // failure, never a silent skip. A manifest platform nobody actually checked must never read as
        // an overall Ok.
        let fx = new_fixture();
        let sig = sign_to_field(&fx.keypair, ARTIFACT);
        let m = manifest(VERSION, URL, &sig);
        let err = verify_update_manifest(&m, &fx.pubkey_config_b64, VERSION, |_| None).unwrap_err();
        assert_eq!(
            err,
            vec![ManifestProblem::ArtifactUnavailable { platform: "windows-x86_64".into() }]
        );
    }

    #[test]
    fn manifest_platform_count_counts_named_platforms() {
        let fx = new_fixture();
        let sig = sign_to_field(&fx.keypair, ARTIFACT);
        let m = manifest(VERSION, URL, &sig);
        assert_eq!(manifest_platform_count(&m), Some(1));

        let two_platform_manifest = serde_json::json!({
            "version": VERSION,
            "platforms": {
                "windows-x86_64": { "signature": sig, "url": URL },
                "linux-x86_64": { "signature": sig, "url": URL },
            }
        })
        .to_string();
        assert_eq!(manifest_platform_count(&two_platform_manifest), Some(2));

        assert_eq!(manifest_platform_count("{ not json "), None);
    }

    const REAL_PREFIX: &str = "https://github.com/StewartScottRogers/cross-platform-explorer/releases/download/v1.2.3/";

    #[test]
    fn url_matching_prefix_is_not_an_offender() {
        let sig = "irrelevant-for-this-check";
        let m = serde_json::json!({
            "version": VERSION,
            "platforms": {
                "windows-x86_64": { "signature": sig, "url": format!("{REAL_PREFIX}app_1.2.3_x64-setup.nsis.zip") }
            }
        })
        .to_string();
        assert_eq!(platforms_with_url_outside_prefix(&m, REAL_PREFIX), Vec::<(String, String)>::new());
    }

    /// CPE-1872 Finding B, `n1_foreign_host_same_basename`: a foreign host serving the exact same
    /// basename a genuine artifact would have. The bytes behind that basename are never fetched from
    /// the foreign host by this pipeline (gh release download only ever pulls from OUR release), but the
    /// manifest ships this url to real updater clients regardless -- must be flagged.
    #[test]
    fn foreign_host_same_basename_is_an_offender() {
        let sig = "irrelevant-for-this-check";
        let evil_url = "https://evil.example/pwn/app_1.2.3_x64-setup.nsis.zip";
        let m = serde_json::json!({
            "version": VERSION,
            "platforms": {
                "windows-x86_64": { "signature": sig, "url": evil_url }
            }
        })
        .to_string();
        assert_eq!(
            platforms_with_url_outside_prefix(&m, REAL_PREFIX),
            vec![("windows-x86_64".to_string(), evil_url.to_string())]
        );
    }

    /// CPE-1872 Finding B, `n2_wrong_tag_same_basename`: right host and repo, WRONG release tag. Same
    /// class of problem -- points a real updater at a release line the current signing/verify pass never
    /// checked -- and must be flagged just like a foreign host.
    #[test]
    fn wrong_tag_same_basename_is_an_offender() {
        let sig = "irrelevant-for-this-check";
        let wrong_tag_url =
            "https://github.com/StewartScottRogers/cross-platform-explorer/releases/download/v0.0.1/app_1.2.3_x64-setup.nsis.zip";
        let m = serde_json::json!({
            "version": VERSION,
            "platforms": {
                "windows-x86_64": { "signature": sig, "url": wrong_tag_url }
            }
        })
        .to_string();
        assert_eq!(
            platforms_with_url_outside_prefix(&m, REAL_PREFIX),
            vec![("windows-x86_64".to_string(), wrong_tag_url.to_string())]
        );
    }

    #[test]
    fn unparseable_manifest_has_no_url_prefix_offenders() {
        // Reported elsewhere (as Unparseable) -- this check has nothing to add on top of that.
        assert_eq!(platforms_with_url_outside_prefix("{ not json ", REAL_PREFIX), Vec::new());
    }

    #[test]
    fn missing_url_field_is_an_offender_with_empty_url() {
        let m = serde_json::json!({
            "version": VERSION,
            "platforms": { "windows-x86_64": { "signature": "sig-only-no-url" } }
        })
        .to_string();
        assert_eq!(
            platforms_with_url_outside_prefix(&m, REAL_PREFIX),
            vec![("windows-x86_64".to_string(), String::new())]
        );
    }
}
