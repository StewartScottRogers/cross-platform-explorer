//! CPE-1923 — binding a manifest platform's **asset** to the release it claims to be part of.
//!
//! # The gap this closes
//!
//! Everything the guard checked before this module was about *agreement between fields*: the
//! manifest's `version` equals `tauri.conf.json`'s `version` (CPE-1058), the configured pubkey
//! equals the in-repo pin (CPE-1873), every `url` starts with this tag's download prefix
//! (CPE-1872 round 3), every signature verifies over the artifact bytes (CPE-1058).
//!
//! Not one of those looks at *which build* the bytes behind a `url` actually are. An independent
//! Security Auditor turned that into a working downgrade against PR #1039's gate:
//!
//! > An actor with **release-asset write only** — a leaked PAT, or any workflow whose
//! > `contents: write` `GITHUB_TOKEN` can be induced to upload; **no signing-key access needed** —
//! > uploads the old, vulnerable `…_0.1.0_x64-setup.exe` **and its genuine old signature** to the
//! > new draft tag, and writes a `latest.json` whose `version` is the new one. Result:
//! > `OK: verified 1 of 1 platform signature(s)`, **EXIT 0**.
//!
//! The signature is real. The pubkey is the pinned one. The url is under the right tag. Every
//! existing check agrees, and published users auto-"update" onto an older signed build — the same
//! outcome CPE-1873's endpoint pin exists to prevent, reached through the **asset** instead of the
//! endpoint.
//!
//! # "Signature valid" is not "an acceptable version to move to"
//!
//! Those are two different questions and this crate used to answer only the first. This module is
//! the **one** place that answers the second, so there is no second copy to disagree with it:
//!
//! * [`bind_signed_artifact`] — the anti-rollback decision. An artifact may only ship under the
//!   version its own **signed** name carries. It reads the minisign trusted comment, not the
//!   uploaded asset's filename: the asset name is attacker-chosen in this exact threat model, and
//!   binding to it was defeated by renaming the upload (SEC-1). The trusted comment is covered by
//!   the global signature, so changing it needs the signing key.
//! * [`platforms_with_wrong_extension_for_key`] — the platform-key → payload-kind decision, so a
//!   `darwin-*` entry cannot serve a Windows installer (denial-of-update, and the exact shape a
//!   channel-mixing bug corrupts).
//!
//! [`crate::verify_update_manifest`]'s [`crate::ManifestProblem::VersionMismatch`] is deliberately
//! **not** a second copy of the anti-rollback rule: that one compares the manifest's `version`
//! field against the config's, which is manifest self-consistency. It says nothing about the bytes,
//! and in the auditor's downgrade it passed.
//!
//! # The macOS naming exception (why "basename must contain the version" is not enough on its own)
//!
//! Tauri names the macOS updater artifact `<productName>.app.tar.gz` — **no version in the
//! name**, and none in the SIGNED name either. Read off this repo's own published `.sig`
//! assets (`v0.57.69-sidecar`), not assumed:
//! ```text
//! platform key         SIGNED name (the trusted comment's `file:` value)
//! ------------------   ------------------------------------------------------------
//! windows-x86_64       Cross-Platform Explorer_0.57.69_x64_en-US.msi        versioned
//! windows-x86_64-nsis  Cross-Platform Explorer_0.57.69_x64-setup.exe        versioned
//! linux-x86_64         Cross-Platform Explorer (Sidecar)_0.57.69_amd64.AppImage  versioned
//! linux-x86_64-rpm     Cross-Platform Explorer (Sidecar)-0.57.69-1.x86_64.rpm    versioned
//! darwin-aarch64       Cross-Platform Explorer (Sidecar).app.tar.gz     NOT versioned
//! darwin-x86_64        Cross-Platform Explorer.app.tar.gz               NOT versioned
//! ```
//!
//! So a blanket rule would break macOS on the first real release, and — checked against the real
//! `.sig` assets — the trusted comment does not rescue it either: macOS's *signed* name is
//! versionless too (`file:Cross-Platform Explorer (Sidecar).app.tar.gz`). There is simply no
//! version anywhere to bind against for this one artifact kind.
//!
//! The exemption is therefore as narrow as the fact that forces it: the platform key must resolve
//! to [`PlatformOs::Darwin`] **and** the artifact's **signed** name must end `.app.tar.gz`. Keying
//! it on the signed name rather than the uploaded one is what makes it safe: an attacker cannot
//! claim it by renaming an upload, so "any signed bytes at all, called `…app.tar.gz`" no longer
//! qualifies — only bytes that were actually signed as a macOS app tarball.
//!
//! The residual is recorded rather than hidden: a genuinely-signed macOS `.app.tar.gz` from a
//! *different release of this same product* is still admitted, bound to this release only by its
//! `url` (CPE-1872 round 3's tag prefix) and its signature. Closing that needs
//! `CFBundleShortVersionString` out of the tarball — tracked as CPE-1942. The binary prints every
//! exemption it grants so a run cannot quietly consist entirely of them.

/// The operating system a manifest platform key names.
///
/// Tauri's keys are `<os>-<arch>` with an optional bundle-kind suffix — `windows-x86_64`,
/// `windows-x86_64-nsis`, `linux-x86_64-rpm`, `darwin-aarch64-app`, … — so the OS is the segment
/// before the first `-`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformOs {
    Windows,
    Darwin,
    Linux,
}

impl std::fmt::Display for PlatformOs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlatformOs::Windows => write!(f, "windows"),
            PlatformOs::Darwin => write!(f, "darwin"),
            PlatformOs::Linux => write!(f, "linux"),
        }
    }
}

/// The updater payload extensions each OS is allowed to serve.
///
/// Both spellings of Tauri's `createUpdaterArtifacts` are permitted: this repo ships `true` (the
/// updater artifact IS the plain installer — `.exe` / `.msi` / `.AppImage`, as the live manifest
/// above shows), while `"v1Compatible"` wraps them (`.nsis.zip` / `.msi.zip` /
/// `.AppImage.tar.gz`). Accepting both means flipping that config setting does not silently red a
/// release, while still refusing anything from a *different* OS's bundler.
const WINDOWS_EXTENSIONS: &[&str] = &[".exe", ".msi", ".nsis.zip", ".msi.zip"];
/// macOS ships exactly one updater payload shape.
const DARWIN_EXTENSIONS: &[&str] = &[".app.tar.gz"];
/// `.AppImage` / `.deb` / `.rpm` are all in this repo's live manifest; `.AppImage.tar.gz` is the
/// `v1Compatible` spelling of the first.
const LINUX_EXTENSIONS: &[&str] = &[".appimage", ".appimage.tar.gz", ".deb", ".rpm"];

impl PlatformOs {
    /// The extensions this OS's bundler is allowed to have produced.
    pub fn allowed_extensions(self) -> &'static [&'static str] {
        match self {
            PlatformOs::Windows => WINDOWS_EXTENSIONS,
            PlatformOs::Darwin => DARWIN_EXTENSIONS,
            PlatformOs::Linux => LINUX_EXTENSIONS,
        }
    }
}

/// Resolve a manifest platform key to the OS it names, or `None` for a key this release process
/// never produces.
///
/// `None` is a **failure** at every call site, never a pass — an unrecognised key is precisely the
/// shape a smuggled platform entry takes (CPE-1872 finding 1), and a rule that shrugs at keys it
/// does not understand is a rule an attacker chooses the key to avoid.
pub fn platform_os_of_key(key: &str) -> Option<PlatformOs> {
    match key.split('-').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "windows" => Some(PlatformOs::Windows),
        // Tauri emits `darwin-*`; `macos-*` is accepted defensively so a future rename of the key
        // does not read as an unknown OS.
        "darwin" | "macos" => Some(PlatformOs::Darwin),
        "linux" => Some(PlatformOs::Linux),
        _ => None,
    }
}

/// The last path segment of an updater `url`, which is the asset's filename on the release.
pub fn basename_of_url(url: &str) -> &str {
    url.rsplit(['/', '\\']).next().unwrap_or(url)
}

/// A normalised, comparable form of a product name or an asset basename: lowercased, with every
/// non-alphanumeric character removed.
///
/// Tauri's bundler mangles `productName` differently per target — `Cross-Platform Explorer
/// (Sidecar)` becomes `Cross-Platform.Explorer.Sidecar._…` in most names and
/// `Cross-Platform.Explorer.Sidecar.-…` in the RPM one — so comparing raw strings means guessing
/// the sanitiser. Normalising both sides removes the guess: `crossplatformexplorersidecar` is a
/// prefix of both real spellings, and of neither plain-channel name.
pub fn product_token(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Why a platform entry's asset is not a payload its own platform key could have produced
/// (CPE-1923 finding 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionFault {
    /// The platform key names no OS this release process builds for.
    UnknownPlatformKey { basename: String },
    /// The asset's extension belongs to a different OS's bundler than the key claims.
    WrongExtension {
        os: PlatformOs,
        basename: String,
        allowed: &'static [&'static str],
    },
}

impl std::fmt::Display for ExtensionFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtensionFault::UnknownPlatformKey { basename } => write!(
                f,
                "platform key names no OS this release builds for (asset `{basename}`); \
                 expected a key beginning `windows-`, `darwin-` or `linux-`"
            ),
            ExtensionFault::WrongExtension { os, basename, allowed } => write!(
                f,
                "serves `{basename}`, which is not a {os} updater payload (expected one of: {})",
                allowed.join(", ")
            ),
        }
    }
}

/// CPE-1923 finding 2 — assert each platform key's asset is a payload that key's OS actually
/// produces.
///
/// The auditor's fixture: a manifest where `darwin-aarch64` serves the Windows installer and
/// `windows-x86_64` serves the macOS `.app.tar.gz`, **each with its own genuine signature**.
/// Channel purity, url prefix and every signature passed — `verified 2 of 2 platform signature(s)`,
/// EXIT 0 — because nothing anywhere related a platform key to the kind of file behind it. The
/// direct outcome is denial-of-update (every client downloads a payload it cannot run), but the
/// platform → asset mapping is exactly what a channel-mixing bug corrupts, so it is worth an
/// assertion of its own rather than being left to whichever guard happens to notice.
///
/// Returns every offending platform with the reason. An unparseable manifest returns no offenders:
/// that failure is reported as [`crate::ManifestProblem::Unparseable`] and this check has nothing
/// to add on top of it.
pub fn platforms_with_wrong_extension_for_key(manifest_json: &str) -> Vec<(String, ExtensionFault)> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(manifest_json) else {
        return Vec::new();
    };
    crate::collect_platforms(&value)
        .into_iter()
        .filter_map(|(name, entry)| {
            let url = entry.get("url").and_then(|u| u.as_str()).unwrap_or("");
            let basename = basename_of_url(url).to_string();
            let Some(os) = platform_os_of_key(&name) else {
                return Some((name, ExtensionFault::UnknownPlatformKey { basename }));
            };
            let lower = basename.to_ascii_lowercase();
            let allowed = os.allowed_extensions();
            if allowed.iter().any(|ext| lower.ends_with(ext)) {
                None
            } else {
                Some((name, ExtensionFault::WrongExtension { os, basename, allowed }))
            }
        })
        .collect()
}

/// The `file:` value out of a **verified** minisign trusted comment.
///
/// `tauri-bundler` signs each artifact with a trusted comment of the form
/// `timestamp:<unix>\tfile:<original filename>` — confirmed by reading the real `.sig` assets off
/// this repo's published `v0.57.69-sidecar` release rather than taken on trust:
///
/// ```text
/// timestamp:1787496720\tfile:Cross-Platform Explorer (Sidecar)_0.57.69_x64-setup.exe
/// timestamp:1787496631\tfile:Cross-Platform Explorer_0.57.69_amd64.deb
/// timestamp:1787496952\tfile:Cross-Platform Explorer (Sidecar)-0.57.69-1.x86_64.rpm
/// timestamp:1787496658\tfile:Cross-Platform Explorer (Sidecar).app.tar.gz
/// ```
///
/// The value can contain spaces (it is the *unsanitised* product name — note `Explorer (Sidecar)_`,
/// which the uploaded asset name never carries), so the field is split on tabs, never whitespace.
/// Returns `None` when there is no `file:` field at all.
///
/// **The literal tab is load-bearing, and that is a deliberate fragility.** If `tauri-bundler` ever
/// emitted a space there instead, this returns `None`, every artifact fails
/// [`SignedBindingFault::NoSignedFilename`], and the release goes red — loudly, on the first tag,
/// rather than silently degrading to "no binding". That is the correct direction for a check whose
/// whole job is refusing what it cannot verify, but it does mean a bundler change to the trusted
/// comment's separator is a release-blocking event to be fixed here, not worked around. Splitting
/// on whitespace instead would *look* more tolerant and would in fact be wrong: the real signed
/// names contain spaces, so a whitespace split truncates `Cross-Platform Explorer (Sidecar)_…` at
/// the first one.
pub fn trusted_comment_file(trusted_comment: &str) -> Option<&str> {
    trusted_comment
        .split('\t')
        .find_map(|field| field.trim().strip_prefix("file:"))
        .map(str::trim)
        .filter(|f| !f.is_empty())
}

/// Why a platform's **signed** artifact does not belong to the release being cut (CPE-1923
/// finding 1, as re-founded after the SEC-1 rename bypass).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignedBindingFault {
    /// The signature carries no usable trusted comment, so there is nothing signature-covered to
    /// bind against. Fails closed: minisign's own `verify` accepts a signature with no trusted
    /// comment at all (`(None, None) => {}`), so treating an absent one as "fine" would hand an
    /// attacker the bypass back by simply stripping it.
    NoSignedFilename { detail: String },
    /// The signed filename does not carry the version this release is shipping. **This is the
    /// downgrade**, and unlike the uploaded asset's name, this one cannot be renamed without the
    /// signing key.
    NotBoundToVersion { signed_file: String, expected_version: String },
    /// The platform key names no OS this release builds for, so the macOS versionless exemption
    /// cannot be evaluated. Fails closed.
    UnknownPlatformKey { signed_file: String },
    /// The release is not shipping any version at all. A missing version is a broken release, not a
    /// permissive one.
    NoExpectedVersion { signed_file: String },
}

impl std::fmt::Display for SignedBindingFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignedBindingFault::NoSignedFilename { detail } => write!(
                f,
                "signature carries no usable `file:` trusted comment ({detail}), so the artifact \
                 cannot be bound to a release -- refusing rather than assuming it is current"
            ),
            SignedBindingFault::NotBoundToVersion { signed_file, expected_version } => write!(
                f,
                "was SIGNED as `{signed_file}`, which is not an artifact of the version being \
                 shipped (`{expected_version}`) -- a correctly-signed build from a DIFFERENT \
                 release is still a downgrade, however the uploaded asset was named"
            ),
            SignedBindingFault::UnknownPlatformKey { signed_file } => write!(
                f,
                "platform key names no OS this release builds for (signed as `{signed_file}`), so \
                 the macOS versionless-artifact exemption cannot be evaluated -- refusing to guess"
            ),
            SignedBindingFault::NoExpectedVersion { signed_file } => write!(
                f,
                "this release declares no version, so `{signed_file}` cannot be bound to one"
            ),
        }
    }
}

/// How a platform's signed artifact was admitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignedBinding {
    /// The signed filename carries the version being shipped.
    BoundToVersion,
    /// Admitted by the narrow macOS exemption: Tauri names the macOS updater artifact
    /// `<productName>.app.tar.gz` with no version in it, and the **trusted comment says the same**
    /// (`file:Cross-Platform Explorer (Sidecar).app.tar.gz`) — so there is no version anywhere to
    /// bind against for this one artifact kind. See [`bind_signed_artifact`] for exactly how narrow
    /// this is and what it still leaves open.
    VersionlessMacApp { signed_file: String },
}

/// CPE-1923 finding 1, **the anti-rollback decision** — the one place that decides whether an
/// artifact belongs to the release being cut.
///
/// # Why this reads the trusted comment and not the asset name (SEC-1)
///
/// The first version of this check bound `expected_version` to the **uploaded asset's filename**.
/// That name is chosen by the attacker in the very threat model this guard declares — release-asset
/// write, no signing key — so the whole binding was defeated by renaming the upload:
///
/// ```text
/// old 0.1.0 installer under its own name            -> refused
/// THE SAME BYTES, THE SAME SIGNATURE, uploaded as
///   Cross-Platform.Explorer.Sidecar._0.57.70_...exe -> EXIT 0, "verified 1 of 1"
/// ```
///
/// Minisign's trusted comment is covered by the **global signature** (`minisign::verify` checks
/// `ed25519::verify(sig_and_trusted_comment, pk, global_sig)`), and `tauri-bundler` writes the
/// original filename into it. So the signed name cannot be altered without the signing key, which
/// is exactly the capability the threat model withholds. Binding to it makes the rename buy
/// nothing.
///
/// **Ordering is load-bearing:** a trusted comment is only trustworthy *after* verification
/// succeeds. This function must never be called on a signature that has not just verified, and its
/// only caller — [`crate::verify_update_manifest`] — calls it immediately after `minisign::verify`
/// returns `Ok` for that same artifact.
///
/// `expected_version` is `tauri.conf.json`'s `version`: the version this checkout is releasing, not
/// the manifest's own `version` field, which the attacker also writes.
///
/// # The macOS exception, and what it still leaves open
///
/// Verified against the real published `.sig` assets: every platform's trusted comment carries the
/// version **except** macOS, whose artifact is genuinely versionless in both the asset name and the
/// signed name (`file:Cross-Platform Explorer (Sidecar).app.tar.gz`). So the trusted comment does
/// **not** remove this exemption — there is no version anywhere to bind against.
///
/// What it does do is shrink the exemption enormously. It is now keyed on the **signed** name, so
/// the artifact must actually have been signed *as a macOS app tarball*. The pre-SEC-1 exemption
/// keyed on the uploaded name, which meant any signed bytes whatsoever — an old Windows `.exe`
/// renamed `…_universal.app.tar.gz` — could claim it. That attack is dead. The residual is now
/// precisely: a genuinely-signed macOS `.app.tar.gz` from a *different release of this same
/// product*, which is CPE-1942 and needs `CFBundleShortVersionString` out of the tarball to close.
pub fn bind_signed_artifact(
    platform: &str,
    trusted_comment: &str,
    expected_version: &str,
) -> Result<SignedBinding, SignedBindingFault> {
    let Some(signed_file) = trusted_comment_file(trusted_comment) else {
        return Err(SignedBindingFault::NoSignedFilename {
            detail: format!("trusted comment was {trusted_comment:?}"),
        });
    };
    let signed_file = signed_file.to_string();

    if expected_version.is_empty() {
        return Err(SignedBindingFault::NoExpectedVersion { signed_file });
    }

    let Some(os) = platform_os_of_key(platform) else {
        return Err(SignedBindingFault::UnknownPlatformKey { signed_file });
    };

    if basename_carries_version(&signed_file, expected_version) {
        return Ok(SignedBinding::BoundToVersion);
    }

    // The narrow macOS exemption -- keyed on the SIGNED name, so it cannot be claimed by renaming
    // an upload. A Windows or Linux key cannot reach it at all.
    if os == PlatformOs::Darwin && signed_file.to_ascii_lowercase().ends_with(".app.tar.gz") {
        return Ok(SignedBinding::VersionlessMacApp { signed_file });
    }

    Err(SignedBindingFault::NotBoundToVersion {
        signed_file,
        expected_version: expected_version.to_string(),
    })
}

/// Does `name` carry `version` as a delimited token?
///
/// "Delimited" means the characters immediately around the match are neither digits nor `.`, so
/// `…_0.57.690_…` does not satisfy a `0.57.69` release and `…_10.57.69_…` does not either. Every
/// real separator Tauri emits around a version (`_`, `-`) satisfies this, as does start/end of the
/// name. An empty `version` matches nothing — fail closed.
fn basename_carries_version(name: &str, version: &str) -> bool {
    if version.is_empty() {
        return false;
    }
    let bytes = name.as_bytes();
    let boundary = |b: u8| !(b.is_ascii_digit() || b == b'.');
    name.match_indices(version).any(|(i, m)| {
        let before_ok = i == 0 || boundary(bytes[i - 1]);
        let end = i + m.len();
        let after_ok = end >= bytes.len() || boundary(bytes[end]);
        before_ok && after_ok
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The platform-key spellings this repo's own live manifest uses, including the suffixed
    /// bundle-kind variants -- read out of `v0.57.69-sidecar`'s published `latest.json`.
    #[test]
    fn platform_keys_from_the_live_manifest_all_resolve() {
        for key in [
            "linux-x86_64",
            "linux-x86_64-appimage",
            "linux-x86_64-deb",
            "linux-x86_64-rpm",
            "darwin-aarch64",
            "darwin-aarch64-app",
            "darwin-x86_64",
            "darwin-x86_64-app",
            "windows-x86_64",
            "windows-x86_64-nsis",
            "windows-x86_64-msi",
        ] {
            assert!(platform_os_of_key(key).is_some(), "{key} must resolve to an OS");
        }
    }

    #[test]
    fn an_unrecognised_platform_key_resolves_to_nothing_rather_than_a_default() {
        assert_eq!(platform_os_of_key("evil-x86_64"), None);
        assert_eq!(platform_os_of_key(""), None);
    }

    #[test]
    fn product_token_collapses_both_bundler_spellings_of_the_sidecar_name() {
        let sidecar = product_token("Cross-Platform Explorer (Sidecar)");
        assert_eq!(sidecar, "crossplatformexplorersidecar");
        // The two spellings the real bundler produces for the SAME product name.
        assert!(product_token("Cross-Platform.Explorer.Sidecar._0.57.69_amd64.AppImage")
            .starts_with(&sidecar));
        assert!(product_token("Cross-Platform.Explorer.Sidecar.-0.57.69-1.x86_64.rpm")
            .starts_with(&sidecar));
        // ...and not for the plain one.
        assert!(!product_token("Cross-Platform.Explorer_0.57.69_x64-setup.exe").starts_with(&sidecar));
    }

    /// The whole live manifest must be clean under the extension check -- otherwise this guard
    /// would red every real release.
    #[test]
    fn the_live_manifests_platform_to_asset_mapping_is_accepted() {
        let m = live_manifest_json();
        assert_eq!(platforms_with_wrong_extension_for_key(&m), Vec::new());
    }

    /// CPE-1923 finding 2, the auditor's fixture: darwin serving the Windows installer and windows
    /// serving the macOS payload. Both must be named.
    #[test]
    fn a_swapped_platform_to_asset_mapping_names_both_platforms() {
        let m = serde_json::json!({
            "version": "0.57.70",
            "platforms": {
                "darwin-aarch64": { "signature": "s", "url": "https://x/Cross-Platform.Explorer.Sidecar._0.57.70_x64-setup.exe" },
                "windows-x86_64": { "signature": "s", "url": "https://x/Cross-Platform.Explorer.Sidecar._aarch64.app.tar.gz" },
            }
        })
        .to_string();
        let mut offenders = platforms_with_wrong_extension_for_key(&m);
        offenders.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(offenders.len(), 2, "both swapped platforms must be named: {offenders:?}");
        assert_eq!(offenders[0].0, "darwin-aarch64");
        assert_eq!(offenders[1].0, "windows-x86_64");
        assert!(matches!(offenders[0].1, ExtensionFault::WrongExtension { os: PlatformOs::Darwin, .. }));
        assert!(matches!(offenders[1].1, ExtensionFault::WrongExtension { os: PlatformOs::Windows, .. }));
    }

    #[test]
    fn an_unknown_platform_key_is_an_extension_offender() {
        let m = serde_json::json!({
            "version": "1.2.3",
            "platforms": { "evil-x86_64": { "signature": "s", "url": "https://x/pwn.exe" } }
        })
        .to_string();
        let offenders = platforms_with_wrong_extension_for_key(&m);
        assert_eq!(offenders.len(), 1);
        assert!(matches!(offenders[0].1, ExtensionFault::UnknownPlatformKey { .. }));
    }

    /// The trusted-comment shape, pinned against the real `.sig` assets published on
    /// `v0.57.69-sidecar`. These strings were read out of those files, not invented — the `file:`
    /// value carries the UNSANITISED product name, with spaces and parentheses the uploaded asset
    /// name never has, which is why the field is split on tabs rather than whitespace.
    #[test]
    fn the_real_trusted_comment_shape_parses() {
        for (comment, expected) in [
            (
                "timestamp:1787496720\tfile:Cross-Platform Explorer (Sidecar)_0.57.69_x64-setup.exe",
                "Cross-Platform Explorer (Sidecar)_0.57.69_x64-setup.exe",
            ),
            (
                "timestamp:1787496952\tfile:Cross-Platform Explorer (Sidecar)-0.57.69-1.x86_64.rpm",
                "Cross-Platform Explorer (Sidecar)-0.57.69-1.x86_64.rpm",
            ),
            (
                "timestamp:1787496658\tfile:Cross-Platform Explorer (Sidecar).app.tar.gz",
                "Cross-Platform Explorer (Sidecar).app.tar.gz",
            ),
            (
                "timestamp:1787496934\tfile:Cross-Platform Explorer.app.tar.gz",
                "Cross-Platform Explorer.app.tar.gz",
            ),
        ] {
            assert_eq!(trusted_comment_file(comment), Some(expected), "{comment}");
        }
    }

    #[test]
    fn a_trusted_comment_with_no_file_field_yields_nothing() {
        assert_eq!(trusted_comment_file("timestamp:1787496720"), None);
        assert_eq!(trusted_comment_file(""), None);
        assert_eq!(trusted_comment_file("timestamp:1\tfile:"), None);
    }

    /// The signed name carries the channel too — and better than the uploaded name does, since it
    /// has the raw `(Sidecar)` marker. `product_token` normalises it to the same token the
    /// sanitised asset name produces.
    #[test]
    fn the_signed_name_normalises_to_the_same_channel_token_as_the_asset_name() {
        let sidecar = product_token("Cross-Platform Explorer (Sidecar)");
        assert!(product_token("Cross-Platform Explorer (Sidecar)_0.57.69_x64-setup.exe").starts_with(&sidecar));
        assert!(product_token("Cross-Platform.Explorer.Sidecar._0.57.69_x64-setup.exe").starts_with(&sidecar));
        assert!(!product_token("Cross-Platform Explorer_0.57.69_x64-setup.exe").starts_with(&sidecar));
    }

    fn tc(file: &str) -> String {
        format!("timestamp:1787496720\tfile:{file}")
    }

    /// CPE-1923 finding 1 / SEC-1: the anti-rollback decision, made from the signed name.
    #[test]
    fn an_artifact_signed_as_the_shipping_version_is_bound() {
        assert_eq!(
            bind_signed_artifact(
                "windows-x86_64",
                &tc("Cross-Platform Explorer (Sidecar)_0.57.70_x64-setup.exe"),
                "0.57.70"
            ),
            Ok(SignedBinding::BoundToVersion)
        );
    }

    /// The downgrade. The UPLOADED name is irrelevant here — this function never sees it, which is
    /// the entire point of the SEC-1 fix.
    #[test]
    fn an_artifact_signed_as_an_older_version_is_refused_however_it_was_uploaded() {
        let fault = bind_signed_artifact(
            "windows-x86_64",
            &tc("Cross-Platform Explorer (Sidecar)_0.1.0_x64-setup.exe"),
            "0.57.70",
        )
        .unwrap_err();
        assert!(matches!(fault, SignedBindingFault::NotBoundToVersion { .. }), "{fault:?}");
        assert!(fault.to_string().contains("was SIGNED as"));
    }

    /// The macOS exemption: forced by the real artifact, which is versionless in the SIGNED name
    /// too — verified against the published `.sig`. This is why the trusted comment could not
    /// delete the exemption outright (CPE-1942 stays open).
    #[test]
    fn a_genuine_versionless_macos_artifact_is_exempt() {
        assert_eq!(
            bind_signed_artifact(
                "darwin-aarch64",
                &tc("Cross-Platform Explorer (Sidecar).app.tar.gz"),
                "0.57.70"
            ),
            Ok(SignedBinding::VersionlessMacApp {
                signed_file: "Cross-Platform Explorer (Sidecar).app.tar.gz".into()
            })
        );
    }

    /// ...but the exemption is keyed on the SIGNED name, so other signed bytes cannot claim it by
    /// being uploaded under a macOS-looking name. This is the attack that worked before SEC-1.
    #[test]
    fn the_exemption_cannot_be_claimed_by_bytes_signed_as_something_else() {
        let fault = bind_signed_artifact(
            "darwin-aarch64",
            // Signed as an old WINDOWS installer; only the upload was renamed, which this function
            // cannot see and does not care about.
            &tc("Cross-Platform Explorer (Sidecar)_0.1.0_x64-setup.exe"),
            "0.57.70",
        )
        .unwrap_err();
        assert!(matches!(fault, SignedBindingFault::NotBoundToVersion { .. }), "{fault:?}");
    }

    /// ...and a non-darwin key cannot claim it even for a genuine macOS artifact.
    #[test]
    fn the_exemption_is_not_available_to_a_non_darwin_key() {
        for key in ["windows-x86_64", "linux-x86_64"] {
            let fault =
                bind_signed_artifact(key, &tc("Cross-Platform Explorer.app.tar.gz"), "0.57.70")
                    .unwrap_err();
            assert!(matches!(fault, SignedBindingFault::NotBoundToVersion { .. }), "{key}: {fault:?}");
        }
    }

    #[test]
    fn a_missing_trusted_comment_file_field_fails_closed() {
        let fault =
            bind_signed_artifact("windows-x86_64", "timestamp:1787496720", "0.57.70").unwrap_err();
        assert!(matches!(fault, SignedBindingFault::NoSignedFilename { .. }), "{fault:?}");
    }

    #[test]
    fn an_unknown_platform_key_fails_closed() {
        let fault = bind_signed_artifact(
            "solaris-sparc",
            &tc("Cross-Platform Explorer (Sidecar)_0.57.70_x64-setup.exe"),
            "0.57.70",
        )
        .unwrap_err();
        assert!(matches!(fault, SignedBindingFault::UnknownPlatformKey { .. }), "{fault:?}");
    }

    #[test]
    fn an_empty_expected_version_fails_closed_rather_than_exempting_everything() {
        let fault = bind_signed_artifact(
            "windows-x86_64",
            &tc("Cross-Platform Explorer (Sidecar)_0.57.70_x64-setup.exe"),
            "",
        )
        .unwrap_err();
        assert!(matches!(fault, SignedBindingFault::NoExpectedVersion { .. }), "{fault:?}");
        // ...including for darwin, which the exemption would otherwise have waved through -- the
        // hole the Reviewer found when `version: ""` reached here.
        let fault = bind_signed_artifact(
            "darwin-aarch64",
            &tc("Cross-Platform Explorer (Sidecar).app.tar.gz"),
            "",
        )
        .unwrap_err();
        assert!(matches!(fault, SignedBindingFault::NoExpectedVersion { .. }), "{fault:?}");
    }

    /// Every artifact of this repo's real published release must bind at its own version, with only
    /// the macOS entries exempt. Signed names read off the actual `.sig` assets.
    #[test]
    fn every_real_published_artifact_binds_at_its_own_version() {
        let cases = [
            ("windows-x86_64", "Cross-Platform Explorer (Sidecar)_0.57.69_x64-setup.exe", false),
            ("windows-x86_64-msi", "Cross-Platform Explorer_0.57.69_x64_en-US.msi", false),
            ("linux-x86_64", "Cross-Platform Explorer (Sidecar)_0.57.69_amd64.AppImage", false),
            ("linux-x86_64-deb", "Cross-Platform Explorer (Sidecar)_0.57.69_amd64.deb", false),
            ("linux-x86_64-rpm", "Cross-Platform Explorer (Sidecar)-0.57.69-1.x86_64.rpm", false),
            ("darwin-aarch64", "Cross-Platform Explorer (Sidecar).app.tar.gz", true),
            ("darwin-x86_64", "Cross-Platform Explorer.app.tar.gz", true),
        ];
        for (platform, signed_file, expect_exempt) in cases {
            let bound = bind_signed_artifact(platform, &tc(signed_file), "0.57.69")
                .unwrap_or_else(|e| panic!("{platform} ({signed_file}) must bind: {e}"));
            let is_exempt = matches!(bound, SignedBinding::VersionlessMacApp { .. });
            assert_eq!(is_exempt, expect_exempt, "{platform} ({signed_file}) exemption mismatch");
        }
    }

    #[test]
    fn the_version_token_must_be_delimited() {
        assert!(basename_carries_version("app_0.57.69_x64-setup.exe", "0.57.69"));
        assert!(basename_carries_version("app-0.57.69-1.x86_64.rpm", "0.57.69"));
        assert!(basename_carries_version("0.57.69", "0.57.69"));
        // A longer version must not be satisfied by a prefix of it, in either direction.
        assert!(!basename_carries_version("app_0.57.690_x64-setup.exe", "0.57.69"));
        assert!(!basename_carries_version("app_10.57.69_x64-setup.exe", "0.57.69"));
        assert!(!basename_carries_version("app_0.57.69.1_x64-setup.exe", "0.57.69"));
        assert!(!basename_carries_version("app_x64-setup.exe", "0.57.69"));
        assert!(!basename_carries_version("app_0.57.69_x64-setup.exe", ""));
    }


    /// The Reviewer's measurement of the SEC-2 anchor regression, pinned as a fixture.
    ///
    /// This repo's **real published** `v0.57.69-sidecar` manifest genuinely contains five
    /// plain-channel entries — that is CPE-1894's live defect, still sitting on the release. The
    /// sidecar job checks it as `--conf` plain + `--expect-channel sidecar`, and the Reviewer
    /// measured what each anchor produces against it:
    ///
    /// ```text
    /// anchor taken from --conf's productName (the regression):  0 offender(s)
    /// anchor taken from the EXPECTED CHANNEL (correct):         5 offender(s)
    /// ```
    ///
    /// Zero, because the plain token is a strict prefix of the sidecar one, so in the
    /// `crate::Channel::Sidecar` arm every plain asset satisfied `starts_with("crossplatformexplorer")`.
    /// `main` catches all five today; shipping the naive anchor would have regressed it.
    ///
    /// Revert `base_product_token` to a raw `product_token(conf_product_name)` and this returns to
    /// 0, which is the assertion below going red.
    #[test]
    fn the_real_published_manifest_yields_exactly_five_channel_offenders_for_the_sidecar_job() {
        // Exactly as `release-sidecar.yml` invokes it: --conf is the BASE (plain) config.
        let offenders = crate::platforms_with_mismatched_channel(
            &live_manifest_json(),
            crate::Channel::Sidecar,
            "Cross-Platform Explorer",
        );
        let mut named: Vec<&str> = offenders.iter().map(|(p, _)| p.as_str()).collect();
        named.sort_unstable();
        assert_eq!(
            named,
            vec![
                "darwin-x86_64",
                "darwin-x86_64-app",
                "windows-x86_64",
                "windows-x86_64-msi",
                "windows-x86_64-nsis",
            ],
            "the five plain-channel entries in the REAL v0.57.69-sidecar manifest must all be \
             named. 0 offenders means the anchor collapsed back to --conf's productName, whose \
             plain token is a prefix of every sidecar token (SEC-2)."
        );
        assert_eq!(offenders.len(), 5);
        for (platform, fault) in &offenders {
            assert_eq!(
                fault,
                &crate::ChannelFault::WrongChannel(crate::Channel::Plain),
                "{platform} must be named as a PLAIN asset, not merely as unrecognised"
            );
        }
    }

    /// The control for the fixture above: checked as the PLAIN channel, the same real manifest's
    /// other six entries are the offenders instead. Proves the check is not simply always-nonempty
    /// on this manifest.
    #[test]
    fn the_same_real_manifest_checked_as_plain_names_the_other_six() {
        let offenders = crate::platforms_with_mismatched_channel(
            &live_manifest_json(),
            crate::Channel::Plain,
            "Cross-Platform Explorer",
        );
        assert_eq!(offenders.len(), 6, "{offenders:?}");
        for (platform, fault) in &offenders {
            assert_eq!(
                fault,
                &crate::ChannelFault::WrongChannel(crate::Channel::Sidecar),
                "{platform} must be named as a SIDECAR asset"
            );
        }
    }

    /// This repo's real `latest.json`, as published on the `v0.57.69-sidecar` release. Reproduced
    /// so the two guards above are pinned against a manifest that actually shipped, not one
    /// written to suit them. Its channel mixing is CPE-1894's live defect, not this module's
    /// concern.
    fn live_manifest_json() -> String {
        let asset = |name: &str| {
            format!("https://github.com/StewartScottRogers/cross-platform-explorer/releases/download/v0.57.69-sidecar/{name}")
        };
        serde_json::json!({
            "version": "0.57.69",
            "platforms": {
                "linux-x86_64": { "signature": "s", "url": asset("Cross-Platform.Explorer.Sidecar._0.57.69_amd64.AppImage") },
                "linux-x86_64-appimage": { "signature": "s", "url": asset("Cross-Platform.Explorer.Sidecar._0.57.69_amd64.AppImage") },
                "linux-x86_64-deb": { "signature": "s", "url": asset("Cross-Platform.Explorer.Sidecar._0.57.69_amd64.deb") },
                "linux-x86_64-rpm": { "signature": "s", "url": asset("Cross-Platform.Explorer.Sidecar.-0.57.69-1.x86_64.rpm") },
                "darwin-aarch64": { "signature": "s", "url": asset("Cross-Platform.Explorer.Sidecar._aarch64.app.tar.gz") },
                "darwin-aarch64-app": { "signature": "s", "url": asset("Cross-Platform.Explorer.Sidecar._aarch64.app.tar.gz") },
                "windows-x86_64": { "signature": "s", "url": asset("Cross-Platform.Explorer_0.57.69_x64_en-US.msi") },
                "windows-x86_64-nsis": { "signature": "s", "url": asset("Cross-Platform.Explorer_0.57.69_x64-setup.exe") },
                "windows-x86_64-msi": { "signature": "s", "url": asset("Cross-Platform.Explorer_0.57.69_x64_en-US.msi") },
                "darwin-x86_64": { "signature": "s", "url": asset("Cross-Platform.Explorer_universal.app.tar.gz") },
                "darwin-x86_64-app": { "signature": "s", "url": asset("Cross-Platform.Explorer_universal.app.tar.gz") },
            }
        })
        .to_string()
    }
}
