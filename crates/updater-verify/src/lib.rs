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

// CPE-1873 — the second, reviewed pin for the updater root-of-trust public key. See
// `pinned_pubkey.rs`'s module doc for what it proves, what it doesn't, and the rotation procedure.
pub mod pinned_pubkey;
pub use pinned_pubkey::{EXPECTED_TAURI_UPDATER_ENDPOINTS, EXPECTED_TAURI_UPDATER_PUBKEY};

// CPE-1903 — the per-platform config files Tauri merges AUTOMATICALLY (no `--config` flag), derived
// from a directory scan rather than enumerated by filename. See that module's doc for why the
// enumeration this replaces failed three rounds running.
pub mod platform_config_guard;
pub use platform_config_guard::{
    is_auto_merged_platform_config_name, platform_config_override_message,
    platform_config_updater_refusal, scan_for_platform_config_updater_overrides,
    PlatformConfigOverride, TAURI_PLATFORM_TOKENS,
};

// CPE-1923 — binding a manifest platform's ASSET to the release it claims to belong to: the
// anti-rollback decision (findings 1) and the platform-key -> payload-kind decision (finding 2).
// See that module's doc for the auditor's signed-downgrade walkthrough and the macOS exception.
pub mod artifact_binding;
pub use artifact_binding::{
    basename_of_url, bind_signed_artifact, platform_os_of_key,
    platforms_with_wrong_extension_for_key, product_token, trusted_comment_file, ExtensionFault,
    PlatformOs, SignedBinding, SignedBindingFault,
};

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
    /// CPE-1923 finding 1 (SEC-1): the signature verified, but the artifact it authenticates does
    /// not belong to the release being cut. "This signature is valid" and "this is an acceptable
    /// version to move to" are different questions; this is the second one, decided from the
    /// signature's own trusted comment rather than from the attacker-chosen upload name.
    ArtifactNotBoundToRelease { platform: String, fault: artifact_binding::SignedBindingFault },
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
                "platform `{platform}` has no locally-available artifact bytes to verify its signature against -- a manifest platform that cannot be verified is a failure, not a skip"
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
            ManifestProblem::ArtifactNotBoundToRelease { platform, fault } => write!(
                f,
                "PROPERTY FAILED -- artifact/version binding (CPE-1923 finding 1, SIGNED DOWNGRADE): platform `{platform}` {fault}"
            ),
        }
    }
}

impl std::error::Error for ManifestProblem {}

/// What a successful [`verify_update_manifest`] actually established, per platform.
///
/// `Ok` used to be `()`. It carries data now because the only *authenticated* name an artifact has
/// is the one inside its verified trusted comment, and callers need it: the binary reports the
/// versionless-macOS exemptions it granted, and runs the channel check a second time over these
/// signature-covered names rather than only over the attacker-chosen upload names.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VerifiedManifest {
    /// `(platform, filename the artifact was SIGNED as)` for every platform that verified.
    pub signed_files: Vec<(String, String)>,
    /// Platforms admitted by the narrow versionless-macOS exemption rather than by carrying the
    /// version. Reported so a run cannot consist silently of exemptions.
    pub versionless_exemptions: Vec<(String, String)>,
}

/// The `file:` value of a trusted comment, or the whole comment when it has no `file:` field.
/// Only reached for comments [`artifact_binding::bind_signed_artifact`] has already accepted, so
/// the fallback is unreachable in practice and exists to keep this total.
fn trusted_comment_file_owned(trusted_comment: &str) -> String {
    artifact_binding::trusted_comment_file(trusted_comment)
        .unwrap_or(trusted_comment)
        .to_string()
}

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
) -> Result<VerifiedManifest, Vec<ManifestProblem>> {
    let mut problems = Vec::new();
    let mut verified = VerifiedManifest::default();

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
            continue;
        }

        // ── CPE-1923 finding 1 / SEC-1: the anti-rollback decision ────────────────────────────
        //
        // Everything above proves the BYTES are ones this key once signed. It says nothing about
        // WHICH RELEASE they are from, and the first version of this guard tried to answer that
        // from the uploaded asset's filename -- which the attacker in this threat model (release
        // asset write, no signing key) simply renames. The signed trusted comment carries the
        // original filename and is covered by the global signature `minisign::verify` just
        // checked, so it is the one name here that an asset-write attacker cannot choose.
        //
        // This runs HERE, inline, immediately after that verification, for two reasons: the
        // trusted comment is only trustworthy once verification has succeeded, and putting the
        // decision inside the function whose `Ok` every caller already gates on means it cannot be
        // forgotten at a call site the way a separate follow-up check could.
        let trusted_comment = match sig_box.trusted_comment() {
            Ok(tc) => tc,
            Err(e) => {
                problems.push(ManifestProblem::ArtifactNotBoundToRelease {
                    platform: name,
                    fault: artifact_binding::SignedBindingFault::NoSignedFilename {
                        detail: e.to_string(),
                    },
                });
                continue;
            }
        };
        match artifact_binding::bind_signed_artifact(&name, &trusted_comment, expected_version) {
            Ok(artifact_binding::SignedBinding::BoundToVersion) => {
                verified.signed_files.push((name, trusted_comment_file_owned(&trusted_comment)));
            }
            Ok(artifact_binding::SignedBinding::VersionlessMacApp { signed_file }) => {
                verified.signed_files.push((name.clone(), signed_file.clone()));
                verified.versionless_exemptions.push((name, signed_file));
            }
            Err(fault) => {
                problems.push(ManifestProblem::ArtifactNotBoundToRelease { platform: name, fault });
            }
        }
    }

    if problems.is_empty() {
        Ok(verified)
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
pub(crate) fn collect_platforms(value: &serde_json::Value) -> Vec<(String, &serde_json::Value)> {
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
            // A bare `starts_with` is NOT enough, and the audit proved it: a url may satisfy the
            // prefix and then climb straight back out of it with dot-segments --
            //   .../releases/download/v1.2.3/../../../../../attacker/evil-repo/releases/download/v1/x.zip
            // resolves, under RFC 3986 / WHATWG dot-segment removal, to a repo any GitHub user can
            // create. Percent-encoding the dots (`%2e%2e/`) is the same bypass wearing a hat.
            //
            // So after the prefix matches, the REMAINDER must be a bare filename: no `/` at all.
            // That kills raw `..`, `%2e%2e`, and every future encoding trick in one stroke, and it
            // costs nothing, because the download step already reduces the url to its last segment
            // and would not have fetched anything else anyway.
            let escapes = match url.strip_prefix(expected_prefix) {
                Some(rest) => rest.contains('/'),
                None => true,
            };
            if escapes {
                Some((name, url))
            } else {
                None
            }
        })
        .collect()
}

/// CPE-1894 — which release **channel** a manifest platform's asset belongs to, inferred from the
/// asset URL's basename. The plain build's installers are named from `tauri.conf.json`'s
/// `productName` ("Cross-Platform Explorer"); the sidecar build's from `tauri.sidecar.conf.json`'s
/// ("Cross-Platform Explorer (Sidecar)", see `release-sidecar.yml`'s header comment) — so a sidecar
/// asset's basename always carries "sidecar" (case-insensitive; Tauri's bundler turns the space into
/// an underscore/dot, e.g. `Cross-Platform.Explorer_(Sidecar)_0.57.69_x64-setup.nsis.zip` vs the
/// plain `Cross-Platform.Explorer_0.57.69_x64-setup.nsis.zip`) and a plain one never does.
///
/// CPE-1908 round 2 (Reviewer, first pass): a Rust-identifier RENAME (`Channel::Sidecar` →
/// `Channel::SidecarBuild`) is a legal, harmless refactor as long as `Display` keeps emitting the
/// same string — nothing about the CLI vocabulary `--expect-channel`/`FromStr` accepts changes. But an
/// earlier version of the TS ratchet read the Rust IDENTIFIER'S spelling (lowercased) as if it WERE
/// the accepted CLI token, so that harmless rename made it go red and then recommend
/// `--expect-channel sidecarbuild` — a value `FromStr` actually REJECTS, which would have broken a
/// real release. `Channel::ALL` + `channel_display_fromstr_round_trip_covers_every_variant` (in
/// `tests`) prove, IN RUST, that `Display`'s output for every variant always parses back via
/// `FromStr` to that same variant, regardless of what the variant's Rust identifier is spelled — and
/// the TS test reads `Display`'s string LITERALS (see `channelPurityCoverage.test.ts`), not the
/// identifiers, so a pure rename is a non-event there too.
///
/// CPE-1908 round 2 (Reviewer, SECOND pass, R2-3): `Channel`, `Channel::ALL`, its `Display` impl, and
/// its `FromStr` impl used to be FOUR independently hand-kept lists of the same two variants, tied
/// together only by a separate `exhaustiveness_guard` match with no `_` wildcard arm. That guard did
/// force a variant + a `Display` arm + a `FromStr` arm to compile — but `ALL`'s own array literal was
/// just sitting next to it, not actually derived from it: adding `Channel::Beta`, a `Display` arm, a
/// `FromStr` arm, and a matching `exhaustiveness_guard` arm all compiled clean while `ALL` silently
/// stayed `[Channel; 2]`, so `channel_display_fromstr_round_trip_covers_every_variant` — which
/// iterates `Channel::ALL`, not the enum itself — then ran ZERO times for the new variant and reported
/// a false PASS, contradicting that comment's own claim.
///
/// Fixed at the root by `define_channel!` below: the enum, `ALL`, `Display`, and `FromStr` are now all
/// generated from ONE macro invocation's token list, in lockstep. There is no second, independently
/// hand-kept list left to drift out of sync — a variant that exists in the enum but not in `ALL` (or
/// vice versa) is no longer a state the compiler can reach, because there is only one place any
/// variant is spelled out at all.
macro_rules! define_channel {
    ( $( $(#[$variant_meta:meta])* $variant:ident => $token:literal ),+ $(,)? ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Channel {
            $( $(#[$variant_meta])* $variant, )+
        }

        impl Channel {
            /// Every `Channel` variant, exactly once — generated by `define_channel!`, not
            /// hand-duplicated (CPE-1908 round 2, R2-3; see the macro's own doc comment above).
            pub const ALL: [Channel; define_channel!(@count $($variant),+)] = [ $( Channel::$variant ),+ ];
        }

        impl std::fmt::Display for Channel {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $( Channel::$variant => write!(f, $token), )+
                }
            }
        }

        /// CPE-1908 — lets a caller (the `--expect-channel` flag on `verify-release-artifacts`) name
        /// which channel it expects EXPLICITLY, instead of only ever deriving it from a `--conf`'s own
        /// `productName`. That derivation assumes `--conf` is a SELF-CONTAINED config carrying
        /// `plugins.updater.pubkey` + `version` + the channel's own `productName` all in one file —
        /// true of the plain channel's `src-tauri/tauri.conf.json`, but NOT of
        /// `tauri.sidecar.conf.json` (a partial overlay: only
        /// `productName`/`identifier`/`bundle.createUpdaterArtifacts`, no `pubkey`, no `version` — see
        /// that file and CPE-1894's own notes on it). So the sidecar verification job reads
        /// pubkey/version/the CPE-1873 pin from the SAME base `tauri.conf.json` the plain job reads
        /// (correct: the sidecar overlay never touches those), while declaring its expected channel
        /// explicitly via this flag rather than by pointing `--conf` at a file that can't answer the
        /// pubkey/version questions on its own.
        impl std::str::FromStr for Channel {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s.to_ascii_lowercase().as_str() {
                    $( $token => Ok(Channel::$variant), )+
                    other => {
                        let known: Vec<String> = [ $( $token ),+ ].iter().map(|t| format!("'{t}'")).collect();
                        Err(format!("unknown channel '{other}' (expected {})", known.join(" or ")))
                    }
                }
            }
        }
    };
    (@count $($x:ident),+) => { [ $( define_channel!(@one $x) ),+ ].len() };
    (@one $x:ident) => { () };
}

define_channel! {
    /// Built from `tauri.sidecar.conf.json` — bundles the AI Console. The standing project rule is
    /// that installs/runs must always use this build (see `[[always-install-sidecar-build]]`).
    Sidecar => "sidecar",
    /// Built from the base `tauri.conf.json` — the plain, sidecar-free explorer.
    Plain => "plain",
}

/// Why a manifest platform's asset does not belong to the channel the manifest declares
/// (CPE-1894, re-founded by CPE-1923 finding 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelFault {
    /// The basename anchors to the OTHER channel's product token. This is CPE-1894's live defect:
    /// a workflow's tag trigger firing on the wrong channel's tag and merging its installers into
    /// this release.
    WrongChannel(Channel),
    /// The basename does not begin with this product's own token at all, so it is not an asset
    /// this product's bundler could have produced — whatever it is, it did not come from the build
    /// this release is verifying.
    NotThisProduct { basename: String },
}

impl std::fmt::Display for ChannelFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelFault::WrongChannel(c) => write!(f, "asset is from the '{c}' channel"),
            ChannelFault::NotThisProduct { basename } => write!(
                f,
                "asset `{basename}` does not begin with this product's own name, so it is not \
                 something this build's bundler produced"
            ),
        }
    }
}

/// CPE-1923 finding 3 — decide, **anchored**, whether a platform's asset belongs to the expected
/// channel.
///
/// This replaces a free substring test. The old rule was
/// `basename.to_ascii_lowercase().contains("sidecar")`, which proves only "the name contains the
/// word sidecar", not "this asset came from the sidecar build". The auditor flipped it in both
/// directions with nothing but release-asset write:
///
/// * a **plain** installer uploaded as `Cross-Platform.Explorer_1.2.3_x64-setup.nsis.zip.sidecar`
///   read as `Channel::Sidecar` and passed a sidecar-channel run at **EXIT 0**;
/// * and any asset whose name simply omits the word passes as plain.
///
/// The anchored rule instead requires the basename to *begin* with the expected product's own
/// name, compared through [`product_token`] so the comparison does not depend on guessing which
/// characters Tauri's bundler replaces (it emits `Cross-Platform.Explorer.Sidecar._…` for most
/// targets and `Cross-Platform.Explorer.Sidecar.-…` for the RPM). A suffix can no longer change
/// what an asset claims to be, in either direction, because only the front of the name is read.
///
/// The plain product name is a strict prefix of the sidecar one, so the plain direction needs one
/// extra clause: a plain-channel manifest must additionally not begin with `<plain>sidecar`. That
/// keeps CPE-1894's original catch (a sidecar asset in a plain manifest) intact. The sidecar
/// direction needs no extra clause — the sidecar token is the longer one and already excludes
/// plain assets by itself.
/// The **base** product identity an asset name is anchored against, with any channel marker
/// stripped — `product_token` minus a trailing `sidecar`.
///
/// CPE-1923 + CPE-1908 interaction, and the reason this exists rather than using the config's
/// `productName` token directly. CPE-1908 made `--expect-channel` authoritative over the channel,
/// and the sidecar job passes **the plain `tauri.conf.json`** (it needs that file's pubkey/version
/// and the CPE-1873 pin, which the sidecar overlay never touches) while checking a pure *sidecar*
/// manifest. So `--conf`'s `productName` and the expected channel legitimately disagree, and an
/// anchor taken straight from `productName` would demand `crossplatformexplorer…` of assets that
/// correctly read `crossplatformexplorersidecar…` — rejecting 100% of real sidecar releases.
///
/// Reducing to the base identity makes the anchor independent of which config was passed: both
/// `Cross-Platform Explorer` and `Cross-Platform Explorer (Sidecar)` yield the same base, and
/// [`channel_product_token`] then re-derives whichever of the two forms the expected channel calls
/// for.
pub fn base_product_token(product_name: &str) -> String {
    let token = product_token(product_name);
    match token.strip_suffix("sidecar") {
        // Guard against a hypothetical product literally named "Sidecar", which would otherwise
        // reduce to an empty anchor and match every asset on earth.
        Some(base) if !base.is_empty() => base.to_string(),
        _ => token,
    }
}

/// The token an asset basename must begin with to belong to `channel`, derived from a base product
/// identity. The sidecar build's `productName` is the plain one with ` (Sidecar)` appended, so its
/// token is the base token with `sidecar` appended.
pub fn channel_product_token(base_token: &str, channel: Channel) -> String {
    match channel {
        Channel::Sidecar => format!("{base_token}sidecar"),
        Channel::Plain => base_token.to_string(),
    }
}

fn channel_fault_of_asset_url(
    url: &str,
    expected: Channel,
    conf_product_name: &str,
) -> Option<ChannelFault> {
    let basename = basename_of_url(url).to_string();
    let base_token = base_product_token(conf_product_name);
    // No product name to anchor against means this check cannot do its job. Fail closed rather
    // than silently degrading to "everything passes" -- the binary refuses before reaching here,
    // and this keeps the function honest on its own terms.
    if base_token.is_empty() {
        return Some(ChannelFault::NotThisProduct { basename });
    }
    let asset_token = product_token(&basename);
    let sidecar_token = channel_product_token(&base_token, Channel::Sidecar);

    // Anchor first: whatever channel is expected, the asset must at least be THIS product's output.
    if !asset_token.starts_with(&base_token) {
        return Some(ChannelFault::NotThisProduct { basename });
    }

    // The plain token is a strict prefix of the sidecar one, so the sidecar marker is what
    // separates them and it is read only at the FRONT of the name. A suffix -- `.sidecar` appended
    // by anyone who can name a release asset -- cannot reach this decision in either direction,
    // which is the whole of CPE-1923 finding 3.
    let actual = if asset_token.starts_with(&sidecar_token) { Channel::Sidecar } else { Channel::Plain };
    if actual == expected {
        None
    } else {
        Some(ChannelFault::WrongChannel(actual))
    }
}

/// Which channel a manifest is EXPECTED to be, derived from the `productName` in the same
/// `tauri.conf.json` (or `tauri.sidecar.conf.json`) a caller is already reading for pubkey/version —
/// so callers never need a separate flag to say which channel they think they're checking.
pub fn expected_channel_from_product_name(product_name: &str) -> Channel {
    if product_name.to_ascii_lowercase().contains("sidecar") {
        Channel::Sidecar
    } else {
        Channel::Plain
    }
}

/// CPE-1894 — the guard this ticket adds: assert every platform a manifest names is built from the
/// SAME release channel as `expected`. This is what made the live bug checkable — `release.yml`'s
/// `v*` tag trigger matched `-sidecar` tags too, so the plain workflow's installers landed in the
/// SAME draft release `release-sidecar.yml` was populating, producing one manifest naming assets
/// from two different products (`linux-x86_64`/`darwin-aarch64` → sidecar, `windows-x86_64`/
/// `darwin-x86_64` → plain, in the actually-published `v0.57.69` manifest) — made checkable
/// deliberately WITHOUT ever reading the workflow YAML that caused it: a test that reads the tag
/// pattern would have agreed with the very pattern that was wrong (see the ticket).
///
/// Returns every platform whose asset does not belong to `expected`, alongside why — empty means
/// the manifest is channel-pure. An unparseable manifest returns no offenders; that failure is
/// already reported elsewhere (as [`ManifestProblem::Unparseable`]), and there is nothing to add on
/// top of it here.
///
/// `expected_product_name` is the `productName` from the same config `expected` was derived from.
/// CPE-1923 finding 3 made it a parameter rather than leaving the check to a free `contains
///("sidecar")` substring test: see `channel_fault_of_asset_url` for what that substring test
/// let through in both directions.
pub fn platforms_with_mismatched_channel(
    manifest_json: &str,
    expected: Channel,
    expected_product_name: &str,
) -> Vec<(String, ChannelFault)> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(manifest_json) else {
        return Vec::new();
    };
    collect_platforms(&value)
        .into_iter()
        .filter_map(|(name, entry)| {
            let url = entry.get("url").and_then(|u| u.as_str()).unwrap_or("");
            channel_fault_of_asset_url(url, expected, expected_product_name)
                .map(|fault| (name, fault))
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
    const URL: &str = "https://example.com/releases/download/v0.57.33/app_0.57.33_x64-setup.nsis.zip";

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
            // CPE-1923/SEC-1: the anti-rollback decision reads the trusted comment's `file:`
            // field, so these fixtures carry the real shape a Tauri signature has. `windows-x86_64`
            // + a versioned name is the ordinary, binding case.
            Some(&format!("timestamp:1787496720	file:app_{VERSION}_x64-setup.exe")),
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

    /// CPE-1872 Finding C1 (round-3 audit): a url may SATISFY the expected prefix and then climb
    /// straight back out of it with dot-segments. Under RFC 3986 / WHATWG dot-segment removal the
    /// url below resolves to `github.com/attacker/evil-repo/...` -- a repo any GitHub user can
    /// create -- while a bare `starts_with` check waves it through. The audit demonstrated this
    /// bypassing the shipped check at EXIT=0 with `OK: verified 1 of 1`.
    ///
    /// Delete the `rest.contains('/')` clause in `platforms_with_url_outside_prefix` and this test
    /// goes red. That is the point of it.
    #[test]
    fn a_url_that_escapes_the_prefix_with_dot_segments_is_an_offender() {
        let sig = "irrelevant-for-this-check";
        let escaping = format!(
            "{REAL_PREFIX}../../../../../attacker/evil-repo/releases/download/v1/app_1.2.3_x64-setup.nsis.zip"
        );
        let m = serde_json::json!({
            "version": VERSION,
            "platforms": { "windows-x86_64": { "signature": sig, "url": escaping } }
        })
        .to_string();
        assert_eq!(
            platforms_with_url_outside_prefix(&m, REAL_PREFIX),
            vec![("windows-x86_64".to_string(), escaping)],
            "a url that satisfies the prefix and then climbs out of it with `..` must be flagged"
        );
    }

    /// Percent-encoding the dots is the same bypass wearing a hat -- and it is why the fix rejects
    /// ANY `/` in the remainder rather than pattern-matching `..`: a rule that hunts for specific
    /// escape spellings loses to the next encoding, while "the remainder must be a bare filename"
    /// does not.
    #[test]
    fn a_percent_encoded_escape_is_also_an_offender() {
        let sig = "irrelevant-for-this-check";
        let escaping =
            format!("{REAL_PREFIX}%2e%2e/%2e%2e/attacker/app_1.2.3_x64-setup.nsis.zip");
        let m = serde_json::json!({
            "version": VERSION,
            "platforms": { "windows-x86_64": { "signature": sig, "url": escaping } }
        })
        .to_string();
        assert_eq!(
            platforms_with_url_outside_prefix(&m, REAL_PREFIX),
            vec![("windows-x86_64".to_string(), escaping)]
        );
    }

    /// The control the two tests above need: a genuine url -- prefix plus a bare filename, no
    /// separators -- must still pass. A refusal that rejects everything is not a fix.
    #[test]
    fn a_genuine_bare_filename_url_is_not_an_offender() {
        let sig = "irrelevant-for-this-check";
        let good = format!("{REAL_PREFIX}app_1.2.3_x64-setup.nsis.zip");
        let m = serde_json::json!({
            "version": VERSION,
            "platforms": { "windows-x86_64": { "signature": sig, "url": good } }
        })
        .to_string();
        assert_eq!(
            platforms_with_url_outside_prefix(&m, REAL_PREFIX),
            Vec::<(String, String)>::new()
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


    // --- CPE-1894 channel purity, re-founded anchored by CPE-1923 finding 3 ---------------------
    //
    // The old inference was `basename.to_ascii_lowercase().contains("sidecar")`, an UNANCHORED
    // substring test. It proves "the name contains the word sidecar", not "this asset came from the
    // sidecar build", so anyone who can name a release asset could flip its apparent channel in
    // either direction. The tests below pin the anchored replacement against the REAL asset names
    // this repo publishes -- read out of `v0.57.69-sidecar`'s own release, not invented -- because
    // the shape the ticket predicted (`Explorer_(Sidecar)_`) is not the shape the bundler emits
    // (`Cross-Platform.Explorer.Sidecar._`), and a guard written to the predicted shape would have
    // rejected every real sidecar release.

    const PLAIN_PRODUCT: &str = "Cross-Platform Explorer";
    const SIDECAR_PRODUCT: &str = "Cross-Platform Explorer (Sidecar)";

    /// Real plain-channel asset names, verbatim from the published release.
    fn plain_assets() -> Vec<&'static str> {
        vec![
            "Cross-Platform.Explorer_0.57.69_x64-setup.exe",
            "Cross-Platform.Explorer_0.57.69_x64_en-US.msi",
            "Cross-Platform.Explorer_0.57.69_amd64.AppImage",
            "Cross-Platform.Explorer_0.57.69_amd64.deb",
            "Cross-Platform.Explorer-0.57.69-1.x86_64.rpm",
            "Cross-Platform.Explorer_universal.app.tar.gz",
        ]
    }

    /// Real sidecar-channel asset names, verbatim from the published release. Note the RPM spells
    /// the product name differently from the rest -- which is exactly why the comparison normalises
    /// instead of guessing the bundler's sanitiser.
    fn sidecar_assets() -> Vec<&'static str> {
        vec![
            "Cross-Platform.Explorer.Sidecar._0.57.69_x64-setup.exe",
            "Cross-Platform.Explorer.Sidecar._0.57.69_amd64.AppImage",
            "Cross-Platform.Explorer.Sidecar._0.57.69_amd64.deb",
            "Cross-Platform.Explorer.Sidecar.-0.57.69-1.x86_64.rpm",
            "Cross-Platform.Explorer.Sidecar._aarch64.app.tar.gz",
        ]
    }

    fn manifest_of(assets: &[&str]) -> String {
        let platforms: serde_json::Map<String, serde_json::Value> = assets
            .iter()
            .enumerate()
            .map(|(i, name)| {
                (
                    format!("windows-x86_64-{i}"),
                    serde_json::json!({
                        "signature": "sig",
                        "url": format!("https://github.com/o/r/releases/download/v0.57.69/{name}")
                    }),
                )
            })
            .collect();
        serde_json::json!({ "version": "0.57.69", "platforms": platforms }).to_string()
    }

    #[test]
    fn expected_channel_from_product_name_reads_plain_and_sidecar() {
        assert_eq!(expected_channel_from_product_name(PLAIN_PRODUCT), Channel::Plain);
        assert_eq!(expected_channel_from_product_name(SIDECAR_PRODUCT), Channel::Sidecar);
        assert_eq!(
            expected_channel_from_product_name("cross-platform explorer (SIDECAR)"),
            Channel::Sidecar
        );
    }

    /// Control: every REAL plain asset name must pass a plain-channel manifest. A guard that reds
    /// the shipping product is not a guard.
    #[test]
    fn every_real_plain_asset_passes_a_plain_manifest() {
        let m = manifest_of(&plain_assets());
        assert_eq!(
            platforms_with_mismatched_channel(&m, Channel::Plain, PLAIN_PRODUCT),
            Vec::new()
        );
    }

    /// Control: every REAL sidecar asset name must pass a sidecar-channel manifest -- including the
    /// RPM's different spelling of the same product name.
    #[test]
    fn every_real_sidecar_asset_passes_a_sidecar_manifest() {
        let m = manifest_of(&sidecar_assets());
        assert_eq!(
            platforms_with_mismatched_channel(&m, Channel::Sidecar, SIDECAR_PRODUCT),
            Vec::new()
        );
    }

    /// CPE-1894's original catch, preserved: a sidecar asset in a plain manifest. The plain product
    /// token is a strict PREFIX of the sidecar one, so this is the case the anchored rule needs its
    /// one extra clause for -- delete that clause and this test goes red.
    #[test]
    fn a_sidecar_asset_in_a_plain_manifest_is_still_named() {
        let m = manifest_of(&["Cross-Platform.Explorer.Sidecar._0.57.69_x64-setup.exe"]);
        assert_eq!(
            platforms_with_mismatched_channel(&m, Channel::Plain, PLAIN_PRODUCT),
            vec![("windows-x86_64-0".to_string(), ChannelFault::WrongChannel(Channel::Sidecar))]
        );
    }

    /// And the reverse: a plain asset in a sidecar manifest.
    #[test]
    fn a_plain_asset_in_a_sidecar_manifest_is_named() {
        let m = manifest_of(&["Cross-Platform.Explorer_0.57.69_x64-setup.exe"]);
        assert_eq!(
            platforms_with_mismatched_channel(&m, Channel::Sidecar, SIDECAR_PRODUCT),
            vec![("windows-x86_64-0".to_string(), ChannelFault::WrongChannel(Channel::Plain))]
        );
    }

    /// CPE-1923 finding 3, the auditor's fixture: a PLAIN installer wearing a `.sidecar` suffix.
    /// The unanchored `contains("sidecar")` rule read this as `Channel::Sidecar` and passed a
    /// sidecar-channel run at EXIT 0. Anchored, the front of the name still says "plain", and a
    /// suffix cannot change that.
    #[test]
    fn a_sidecar_suffixed_plain_asset_cannot_pass_as_sidecar() {
        let m = manifest_of(&["Cross-Platform.Explorer_1.2.3_x64-setup.nsis.zip.sidecar"]);
        assert_eq!(
            platforms_with_mismatched_channel(&m, Channel::Sidecar, SIDECAR_PRODUCT),
            vec![("windows-x86_64-0".to_string(), ChannelFault::WrongChannel(Channel::Plain))],
            "a trailing `.sidecar` must not buy a plain asset a sidecar channel"
        );
    }

    /// The same trick in the other direction: an asset that is not this product's output at all.
    /// Anchored, it fails to match the product's own token -- it is simply not a name this
    /// bundler produces.
    #[test]
    fn an_asset_that_is_not_this_products_output_at_all_is_named_as_such() {
        let m = manifest_of(&["pwn_1.2.3_x64-setup.exe"]);
        assert_eq!(
            platforms_with_mismatched_channel(&m, Channel::Plain, PLAIN_PRODUCT),
            vec![(
                "windows-x86_64-0".to_string(),
                ChannelFault::NotThisProduct { basename: "pwn_1.2.3_x64-setup.exe".into() }
            )]
        );
    }

    /// A missing `productName` leaves nothing to anchor against. Fail closed, never open. (The
    /// binary refuses earlier; this keeps the function honest on its own terms.)
    #[test]
    fn an_empty_product_name_fails_closed() {
        let m = manifest_of(&["Cross-Platform.Explorer_0.57.69_x64-setup.exe"]);
        assert_eq!(platforms_with_mismatched_channel(&m, Channel::Plain, "").len(), 1);
    }

    #[test]
    fn unparseable_manifest_has_no_channel_offenders() {
        // Reported elsewhere (as Unparseable) -- this check has nothing to add on top of that.
        assert_eq!(
            platforms_with_mismatched_channel("{ not json ", Channel::Plain, PLAIN_PRODUCT),
            Vec::new()
        );
    }

    // --- CPE-1908: Channel::FromStr, backing --expect-channel ---------------------------------

    #[test]
    fn channel_from_str_accepts_plain_and_sidecar_case_insensitively() {
        assert_eq!("plain".parse::<Channel>(), Ok(Channel::Plain));
        assert_eq!("Plain".parse::<Channel>(), Ok(Channel::Plain));
        assert_eq!("PLAIN".parse::<Channel>(), Ok(Channel::Plain));
        assert_eq!("sidecar".parse::<Channel>(), Ok(Channel::Sidecar));
        assert_eq!("Sidecar".parse::<Channel>(), Ok(Channel::Sidecar));
        assert_eq!("SIDECAR".parse::<Channel>(), Ok(Channel::Sidecar));
    }

    #[test]
    fn channel_from_str_rejects_anything_else() {
        assert!("plaintext".parse::<Channel>().is_err());
        assert!("".parse::<Channel>().is_err());
        assert!("plain-sidecar".parse::<Channel>().is_err());
    }

    /// CPE-1908 round 2 (Reviewer) — proves, for EVERY `Channel::ALL` entry, that `Display`'s output
    /// parses back via `FromStr` to the same variant. This is what makes a pure Rust-identifier
    /// rename (e.g. `Sidecar` → `SidecarBuild`) provably safe for the CLI vocabulary: as long as this
    /// keeps passing, `Display`/`FromStr` stay each other's inverse regardless of what the variant is
    /// spelled, so nothing downstream that reads the string tokens (this test, the TS ratchet, the
    /// real `--expect-channel` flag) can be broken by a rename alone.
    ///
    /// R2-3 correction: this comment used to claim `Channel::ALL` was "fed by `exhaustiveness_guard`'s
    /// compile-enforced match, so this loop can never silently skip a variant" — that was FALSE. The
    /// old `exhaustiveness_guard` only forced a *match arm* to exist per variant; `ALL`'s own literal
    /// was a separate, hand-kept list nothing tied it to, so a variant plus a `Display` arm plus a
    /// `FromStr` arm plus a guard arm all compiled clean while `ALL` silently stayed `[Channel; 2]`.
    /// THIS loop iterates `Channel::ALL`, not the enum itself, so it then ran zero times for the new
    /// variant and reported green.
    ///
    /// Fixed at the root by `define_channel!` (see `Channel`'s own doc comment): the enum, `ALL`,
    /// `Display`, and `FromStr` are now all generated from the SAME macro invocation, so there is no
    /// second, independently hand-kept list left for `ALL` to silently fall behind. A variant that
    /// exists in the enum but not in `ALL` is no longer a state the compiler can reach, which is what
    /// makes this loop's coverage claim true now instead of merely asserted.
    #[test]
    fn channel_display_fromstr_round_trip_covers_every_variant() {
        for c in Channel::ALL {
            let token = c.to_string();
            let parsed: Channel = token.parse().unwrap_or_else(|e| {
                panic!("Channel::{c:?}'s Display output {token:?} did not parse back via FromStr: {e}")
            });
            assert_eq!(parsed, c, "Display->FromStr round trip must return the SAME variant");
        }
    }
}
