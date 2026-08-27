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
//! * [`platforms_not_bound_to_version`] — the anti-rollback decision. An asset may only ship under
//!   the version whose token its own filename carries.
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
//! Tauri names the macOS updater artifact `<productName>.app.tar.gz` — **no version in the name**.
//! Confirmed against this repo's own published releases (`v0.57.69-sidecar`):
//!
//! ```text
//! windows-x86_64        Cross-Platform.Explorer_0.57.69_x64_en-US.msi           <- versioned
//! windows-x86_64-nsis   Cross-Platform.Explorer_0.57.69_x64-setup.exe           <- versioned
//! linux-x86_64          Cross-Platform.Explorer.Sidecar._0.57.69_amd64.AppImage <- versioned
//! linux-x86_64-rpm      Cross-Platform.Explorer.Sidecar.-0.57.69-1.x86_64.rpm   <- versioned
//! darwin-aarch64        Cross-Platform.Explorer.Sidecar._aarch64.app.tar.gz     <- NOT versioned
//! ```
//!
//! So a blanket rule would break macOS on the first real release. The exemption is deliberately as
//! narrow as the fact that forces it: the platform key must resolve to [`PlatformOs::Darwin`] **and**
//! the basename must end in `.app.tar.gz`. A Windows or Linux entry cannot claim it by renaming its
//! payload, and a `darwin-*` entry that claims it is simultaneously held to
//! [`platforms_with_wrong_extension_for_key`], which permits `darwin-*` nothing else. The residual
//! is honest and recorded rather than hidden: a macOS `.app.tar.gz` is still bound to the release
//! only by its `url` (CPE-1872 round 3's tag prefix) and its signature, not by its own name. The
//! binary prints every exemption it grants so a run cannot quietly consist entirely of them.

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

/// Why a platform entry's asset is not bound to the version being shipped (CPE-1923 finding 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionBindingFault {
    /// The platform key names no OS this release builds for, so the macOS versionless exemption
    /// cannot even be evaluated. Fails closed.
    UnknownPlatformKey { basename: String },
    /// The asset's filename carries no occurrence of the version this release is shipping. This is
    /// the auditor's downgrade: a genuinely-signed *older* installer uploaded under the new tag.
    NotBoundToVersion { basename: String, expected_version: String },
}

impl std::fmt::Display for VersionBindingFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionBindingFault::UnknownPlatformKey { basename } => write!(
                f,
                "platform key names no OS this release builds for (asset `{basename}`), so the \
                 macOS versionless-artifact exemption cannot be evaluated -- refusing to guess"
            ),
            VersionBindingFault::NotBoundToVersion { basename, expected_version } => write!(
                f,
                "serves `{basename}`, whose filename does not carry the version being shipped \
                 (`{expected_version}`) -- a correctly-signed artifact from a DIFFERENT release is \
                 still a downgrade"
            ),
        }
    }
}

/// A platform let through by the narrow macOS exemption rather than by actually carrying the
/// version. Reported so a run cannot consist silently of exemptions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionlessExemption {
    pub platform: String,
    pub basename: String,
}

/// The outcome of the anti-rollback decision over a whole manifest.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VersionBinding {
    /// Platforms whose asset is not bound to the version being shipped. Non-empty means refuse.
    pub offenders: Vec<(String, VersionBindingFault)>,
    /// Platforms let through by the macOS versionless-artifact exemption.
    pub exemptions: Vec<VersionlessExemption>,
}

/// CPE-1923 finding 1, **the anti-rollback decision** — the one place that decides whether a
/// manifest's artifacts belong to the version being shipped.
///
/// `expected_version` is `tauri.conf.json`'s `version`: the version this checkout is actually
/// releasing, not the manifest's own claim about itself. Using the config's value is what makes
/// this independent of the manifest — an attacker who can write `latest.json` controls the
/// manifest's `version` field, and binding artifact names to a field the attacker also writes
/// would prove nothing.
///
/// An empty `expected_version` makes every platform an offender rather than exempting all of them:
/// a missing version is a broken release, not a permissive one. (The binary refuses before
/// reaching here, but this function must be safe on its own terms.)
///
/// The version must appear as a **delimited token**, so shipping `0.57.69` does not satisfy a
/// manifest for `0.57.6`, and `0.57.690` does not satisfy one for `0.57.69`.
pub fn platforms_not_bound_to_version(manifest_json: &str, expected_version: &str) -> VersionBinding {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(manifest_json) else {
        return VersionBinding::default();
    };
    let mut out = VersionBinding::default();
    for (name, entry) in crate::collect_platforms(&value) {
        let url = entry.get("url").and_then(|u| u.as_str()).unwrap_or("");
        let basename = basename_of_url(url).to_string();

        let Some(os) = platform_os_of_key(&name) else {
            out.offenders
                .push((name, VersionBindingFault::UnknownPlatformKey { basename }));
            continue;
        };

        if basename_carries_version(&basename, expected_version) {
            continue;
        }

        // The narrow macOS exemption -- see this module's doc. `darwin-*` AND `.app.tar.gz`; a
        // Windows/Linux key cannot reach it by renaming its payload, and a `darwin-*` key that
        // reaches it has already been held to `platforms_with_wrong_extension_for_key`, which
        // permits `darwin-*` nothing but `.app.tar.gz` anyway.
        if os == PlatformOs::Darwin && basename.to_ascii_lowercase().ends_with(".app.tar.gz") {
            out.exemptions.push(VersionlessExemption { platform: name, basename });
            continue;
        }

        out.offenders.push((
            name,
            VersionBindingFault::NotBoundToVersion {
                basename,
                expected_version: expected_version.to_string(),
            },
        ));
    }
    out
}

/// Does `basename` carry `version` as a delimited token?
///
/// "Delimited" means the characters immediately around the match are neither digits nor `.`, so
/// `…_0.57.690_…` does not satisfy a `0.57.69` release and `…_10.57.69_…` does not either. Every
/// real separator Tauri emits around a version (`_`, `-`) satisfies this, as does start/end of the
/// name. An empty `version` matches nothing — fail closed.
fn basename_carries_version(basename: &str, version: &str) -> bool {
    if version.is_empty() {
        return false;
    }
    let bytes = basename.as_bytes();
    let boundary = |b: u8| !(b.is_ascii_digit() || b == b'.');
    basename.match_indices(version).any(|(i, m)| {
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

    /// The whole live manifest must bind clean at its own version, exempting exactly the
    /// versionless macOS assets -- otherwise this guard would red every real release.
    #[test]
    fn the_live_manifest_binds_to_its_own_version_with_only_the_macos_entries_exempt() {
        let binding = platforms_not_bound_to_version(&live_manifest_json(), "0.57.69");
        assert_eq!(binding.offenders, Vec::new(), "no offenders expected: {binding:?}");
        let mut exempt: Vec<&str> = binding.exemptions.iter().map(|e| e.platform.as_str()).collect();
        exempt.sort_unstable();
        assert_eq!(
            exempt,
            vec!["darwin-aarch64", "darwin-aarch64-app", "darwin-x86_64", "darwin-x86_64-app"],
            "only the versionless macOS .app.tar.gz entries may be exempt"
        );
    }

    /// CPE-1923 finding 1, the serious one: a genuinely-signed OLDER installer uploaded under the
    /// new tag. The name does not carry the version being shipped, so it must be refused.
    #[test]
    fn a_signed_downgrade_is_refused_by_name() {
        let m = serde_json::json!({
            "version": "0.57.70",
            "platforms": {
                "windows-x86_64": { "signature": "s", "url": "https://x/Cross-Platform.Explorer.Sidecar._0.1.0_x64-setup.exe" }
            }
        })
        .to_string();
        let binding = platforms_not_bound_to_version(&m, "0.57.70");
        assert_eq!(binding.offenders.len(), 1, "{binding:?}");
        assert_eq!(binding.offenders[0].0, "windows-x86_64");
        assert!(matches!(binding.offenders[0].1, VersionBindingFault::NotBoundToVersion { .. }));
        assert!(binding.exemptions.is_empty());
    }

    /// The exemption is for macOS's naming, not for anyone who wants it: a Windows key cannot buy
    /// its way out of the version binding by naming its payload `.app.tar.gz`.
    #[test]
    fn the_versionless_exemption_cannot_be_claimed_by_a_non_darwin_key() {
        for key in ["windows-x86_64", "linux-x86_64"] {
            let m = serde_json::json!({
                "version": "0.57.70",
                "platforms": { key: { "signature": "s", "url": "https://x/Cross-Platform.Explorer_universal.app.tar.gz" } }
            })
            .to_string();
            let binding = platforms_not_bound_to_version(&m, "0.57.70");
            assert_eq!(binding.offenders.len(), 1, "{key} must not be exempt: {binding:?}");
            assert!(binding.exemptions.is_empty(), "{key} must not be exempt: {binding:?}");
        }
    }

    /// ...and a `darwin-*` key cannot claim it for something that is not the versionless macOS
    /// payload either.
    #[test]
    fn a_darwin_key_serving_a_versioned_payload_is_still_bound_to_the_version() {
        let m = serde_json::json!({
            "version": "0.57.70",
            "platforms": {
                "darwin-aarch64": { "signature": "s", "url": "https://x/Cross-Platform.Explorer.Sidecar._0.1.0_aarch64.dmg" }
            }
        })
        .to_string();
        let binding = platforms_not_bound_to_version(&m, "0.57.70");
        assert_eq!(binding.offenders.len(), 1, "{binding:?}");
        assert!(binding.exemptions.is_empty());
    }

    #[test]
    fn an_empty_expected_version_fails_closed_rather_than_exempting_everything() {
        let m = serde_json::json!({
            "version": "",
            "platforms": {
                "windows-x86_64": { "signature": "s", "url": "https://x/Cross-Platform.Explorer_0.57.69_x64-setup.exe" }
            }
        })
        .to_string();
        let binding = platforms_not_bound_to_version(&m, "");
        assert_eq!(binding.offenders.len(), 1, "an empty version must refuse, not permit: {binding:?}");
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
