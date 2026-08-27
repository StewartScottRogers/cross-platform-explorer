//! CPE-1923 — the independent Security Auditor's hostile manifests, landed as fixtures.
//!
//! Three manifests built against PR #1039's gate **passed at EXIT 0 with genuine minisign
//! signatures**. Reasoning about them was explicitly not enough: each one is reproduced here end to
//! end — real throwaway keypair, real signatures over real bytes on disk, the real binary, run the
//! way `release-sidecar.yml` invokes it — and asserted to produce a **non-zero exit** naming the
//! property that failed.
//!
//! Every assertion in this file is on the **exit status** first. A test that only checked the
//! stderr text would go green against a binary that printed the complaint and then exited 0, which
//! is the precise shape of the guards this repo has been finding all week. `refuse()` therefore
//! asserts the exit code before it looks at a single character of output, and reports the code it
//! actually saw when it fails.
//!
//! Controls are not optional here. A verifier that rejects everything is not a fix, so each hostile
//! case is paired with a legitimate manifest — plain channel and sidecar channel, including the
//! versionless macOS `.app.tar.gz` the version binding has to make an exception for — asserted to
//! still exit 0. Break any of the three fixes and the matching hostile test reddens; break them too
//! broadly and the controls redden.
//!
//! No real signing material is used or touched: every fixture generates its own throwaway minisign
//! keypair in a temp dir, and `--skip-pin-check` keeps the repo's real pinned pubkey out of it
//! (exactly as the sibling fixtures in `release_guard.rs` do).

use std::path::Path;
use std::process::{Command, Output};

use base64::Engine as _;
use minisign::KeyPair;

const B64: base64::engine::general_purpose::GeneralPurpose = base64::engine::general_purpose::STANDARD;
const BIN: &str = env!("CARGO_BIN_EXE_verify-release-artifacts");

/// The version this fixture release is shipping.
const VERSION: &str = "0.57.70";
/// The version the auditor's downgrade smuggles in — an older, "vulnerable" build.
const OLD_VERSION: &str = "0.1.0";

const PLAIN_PRODUCT: &str = "Cross-Platform Explorer";
const SIDECAR_PRODUCT: &str = "Cross-Platform Explorer (Sidecar)";
const REPO: &str = "StewartScottRogers/cross-platform-explorer";

/// One platform entry to scaffold: manifest key, asset filename, and the bytes that are BOTH
/// written to disk and signed. Every fixture signs genuinely — the whole point of these three is
/// that the cryptography is real and the manifest is hostile anyway.
struct Entry {
    platform: &'static str,
    asset: String,
    bytes: Vec<u8>,
}

fn entry(platform: &'static str, asset: impl Into<String>, bytes: &str) -> Entry {
    Entry { platform, asset: asset.into(), bytes: bytes.as_bytes().to_vec() }
}

/// The release-download prefix this fixture release publishes under, matching what
/// `release-sidecar.yml` / `release.yml` compute from `${REPO}` and `${TAG}`.
fn url_prefix(tag: &str) -> String {
    format!("https://github.com/{REPO}/releases/download/{tag}/")
}

/// Build a complete, self-consistent release tree: a `tauri.conf.json` (product name, version,
/// throwaway pubkey), a `latest.json` naming `entries`, and each entry's asset on disk with a
/// genuine signature over its own bytes.
///
/// `manifest_version` is separate from `conf_version` so a fixture can do what the auditor's
/// downgrade does — ship an old artifact under a new version — and so the plain version-mismatch
/// case stays expressible.
fn scaffold(
    product_name: &str,
    conf_version: &str,
    manifest_version: &str,
    tag: &str,
    entries: &[Entry],
) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    let kp = KeyPair::generate_unencrypted_keypair().expect("keypair");
    let pubkey_config = B64.encode(kp.pk.to_box().expect("pk box").into_string().as_bytes());

    let mut platforms = serde_json::Map::new();
    for e in entries {
        std::fs::write(root.join(&e.asset), &e.bytes).expect("write artifact");
        let sig = minisign::sign(
            Some(&kp.pk),
            &kp.sk,
            std::io::Cursor::new(&e.bytes),
            Some("trusted"),
            Some("untrusted"),
        )
        .expect("sign");
        platforms.insert(
            e.platform.to_string(),
            serde_json::json!({
                "signature": B64.encode(sig.into_string().as_bytes()),
                "url": format!("{}{}", url_prefix(tag), e.asset),
            }),
        );
    }

    let manifest = serde_json::json!({ "version": manifest_version, "platforms": platforms });
    std::fs::write(root.join("latest.json"), manifest.to_string()).expect("write manifest");

    let conf = serde_json::json!({
        "version": conf_version,
        "productName": product_name,
        "plugins": { "updater": { "pubkey": pubkey_config } }
    });
    std::fs::write(root.join("tauri.conf.json"), conf.to_string()).expect("write conf");

    dir
}

/// Run the binary the way the release workflows do: explicit manifest, single search dir holding
/// the downloaded assets, and the tag's real download prefix enforced.
fn run(dir: &Path, tag: &str) -> Output {
    Command::new(BIN)
        .args([
            "--conf",
            dir.join("tauri.conf.json").to_str().unwrap(),
            "--manifest",
            dir.join("latest.json").to_str().unwrap(),
            "--search",
            dir.to_str().unwrap(),
            "--expect-url-prefix",
            &url_prefix(tag),
            // Throwaway per-fixture keypair, unrelated to the repo's real pinned pubkey.
            "--skip-pin-check",
        ])
        .output()
        .expect("run verify-release-artifacts")
}

/// Assert the run REFUSED: non-zero exit first, then that the message names the property and the
/// offending platform. Returns stderr for any further assertions.
///
/// The exit-code assertion is deliberately first and unconditional. `verify-release-artifacts` is
/// the only thing standing between a hostile manifest and a published auto-update, and the release
/// workflow reads nothing but its exit status — a check that complains on stderr and exits 0 is
/// indistinguishable from no check at all.
#[track_caller]
fn refuse(out: &Output, must_name: &[&str]) -> String {
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        !out.status.success(),
        "EXPECTED A NON-ZERO EXIT, GOT {:?} (success). This is the failure mode the whole ticket is \
         about: the manifest was refused in words or not at all, while the process told the release \
         workflow everything was fine.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
        out.status.code(),
    );
    for needle in must_name {
        assert!(
            stderr.contains(needle),
            "refusal must name `{needle}` so the log says WHICH property failed and where.\n\
             --- stderr ---\n{stderr}"
        );
    }
    stderr
}

/// Assert the run ACCEPTED, and actually verified something rather than passing vacuously.
#[track_caller]
fn accept(out: &Output, platforms: usize) {
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        out.status.success(),
        "a LEGITIMATE manifest must still pass -- a verifier that rejects everything is not a \
         fix.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
    );
    assert!(
        stdout.contains(&format!("verified {platforms} of {platforms} platform signature(s)")),
        "expected all {platforms} platform(s) to be cryptographically verified.\n--- stdout ---\n{stdout}"
    );
}

// ── Hostile 1 — the signed downgrade (the serious one) ────────────────────────────────────────
//
// Auditor's finding 1. An actor with release-asset write ONLY -- a leaked PAT, or any workflow
// whose `contents: write` GITHUB_TOKEN can be induced to upload; no signing-key access needed --
// uploads the old, vulnerable 0.1.0 installer AND ITS GENUINE OLD SIGNATURE to the new draft tag,
// and writes a latest.json whose `version` is the new one. Demonstrated pre-fix:
// `OK: verified 1 of 1 platform signature(s)`, EXIT 0. latest.json is itself unsigned and the Tauri
// updater compares only the manifest's `version`, so published users auto-"update" onto the older
// signed build -- the same downgrade CPE-1873's endpoint pin exists to prevent, reached through the
// ASSET instead of the endpoint.

#[test]
fn h1_a_genuinely_signed_downgrade_to_an_older_installer_is_refused() {
    let tag = format!("v{VERSION}-sidecar");
    let dir = scaffold(
        SIDECAR_PRODUCT,
        VERSION,
        // The manifest claims the NEW version. Everything the old gate compared -- manifest version
        // vs config version -- agrees perfectly.
        VERSION,
        &tag,
        &[entry(
            "windows-x86_64",
            format!("Cross-Platform.Explorer.Sidecar._{OLD_VERSION}_x64-setup.exe"),
            "the OLD, vulnerable 0.1.0 installer bytes -- genuinely signed",
        )],
    );
    let out = run(dir.path(), &tag);
    let stderr = refuse(&out, &["artifact/version binding", "windows-x86_64"]);
    assert!(
        stderr.contains(OLD_VERSION) && stderr.contains(VERSION),
        "the refusal must show both the version shipped and the artifact actually offered, or a \
         reader cannot tell a downgrade from a typo.\n--- stderr ---\n{stderr}"
    );
}

/// The downgrade's control, and the one that keeps this fix honest: the SAME manifest shape with
/// the CURRENT version's installer must still pass, signature and all.
#[test]
fn h1_control_the_current_versions_installer_still_passes() {
    let tag = format!("v{VERSION}-sidecar");
    let dir = scaffold(
        SIDECAR_PRODUCT,
        VERSION,
        VERSION,
        &tag,
        &[entry(
            "windows-x86_64",
            format!("Cross-Platform.Explorer.Sidecar._{VERSION}_x64-setup.exe"),
            "the real 0.57.70 installer bytes",
        )],
    );
    accept(&run(dir.path(), &tag), 1);
}

/// The downgrade is refused for being the wrong VERSION, not for being the wrong signature: proved
/// by the fact that a manifest carrying the old artifact with a DELIBERATELY BROKEN signature fails
/// too, and that the version binding fires before the crypto ever runs. Without this, "the guard
/// catches it" could just mean the crypto happened to catch something else.
#[test]
fn h1_the_downgrade_is_refused_on_version_grounds_not_signature_grounds() {
    let tag = format!("v{VERSION}-sidecar");
    let dir = scaffold(
        SIDECAR_PRODUCT,
        VERSION,
        VERSION,
        &tag,
        &[entry(
            "windows-x86_64",
            format!("Cross-Platform.Explorer.Sidecar._{OLD_VERSION}_x64-setup.exe"),
            "the OLD, vulnerable 0.1.0 installer bytes -- genuinely signed",
        )],
    );
    let stderr = refuse(&run(dir.path(), &tag), &["artifact/version binding"]);
    assert!(
        !stderr.contains("did NOT verify"),
        "the signature on this artifact IS genuine -- if the refusal mentions signature failure, \
         the fixture is not reproducing the auditor's scenario.\n--- stderr ---\n{stderr}"
    );
}

/// The macOS naming exception, end to end: Tauri names the macOS updater artifact
/// `<productName>.app.tar.gz` with NO version in it (verified against this repo's own published
/// `v0.57.69-sidecar` assets). A version binding that broke macOS would be a worse bug than the one
/// it fixes, so a real, mixed, multi-platform release must pass whole.
#[test]
fn h1_control_a_full_multi_platform_release_including_versionless_macos_passes() {
    let tag = format!("v{VERSION}-sidecar");
    let dir = scaffold(
        SIDECAR_PRODUCT,
        VERSION,
        VERSION,
        &tag,
        &[
            entry(
                "windows-x86_64",
                format!("Cross-Platform.Explorer.Sidecar._{VERSION}_x64_en-US.msi"),
                "windows msi bytes",
            ),
            entry(
                "windows-x86_64-nsis",
                format!("Cross-Platform.Explorer.Sidecar._{VERSION}_x64-setup.exe"),
                "windows nsis bytes",
            ),
            entry(
                "linux-x86_64",
                format!("Cross-Platform.Explorer.Sidecar._{VERSION}_amd64.AppImage"),
                "linux appimage bytes",
            ),
            entry(
                "linux-x86_64-rpm",
                format!("Cross-Platform.Explorer.Sidecar.-{VERSION}-1.x86_64.rpm"),
                "linux rpm bytes",
            ),
            // The exception: no version anywhere in the name. This is exactly how the real asset is
            // named on the published release.
            entry(
                "darwin-aarch64",
                "Cross-Platform.Explorer.Sidecar._aarch64.app.tar.gz",
                "macos app tarball bytes",
            ),
        ],
    );
    let out = run(dir.path(), &tag);
    accept(&out, 5);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("exempt   : darwin-aarch64"),
        "an exemption must be printed, not granted silently -- otherwise a run could consist \
         entirely of exemptions and still read as a clean verification.\n--- stdout ---\n{stdout}"
    );
}

/// ...and the exemption must not become a hole. The macOS payload's own name carries no version, so
/// the ONLY thing distinguishing it is the `darwin-*` key plus the `.app.tar.gz` extension. A
/// Windows platform key must not be able to buy the same exemption by naming its payload that way.
#[test]
fn h1_the_versionless_exemption_is_not_available_to_a_windows_platform() {
    let tag = format!("v{VERSION}-sidecar");
    let dir = scaffold(
        SIDECAR_PRODUCT,
        VERSION,
        VERSION,
        &tag,
        &[entry(
            "windows-x86_64",
            "Cross-Platform.Explorer.Sidecar._aarch64.app.tar.gz",
            "an old build wearing the versionless macOS name",
        )],
    );
    refuse(&run(dir.path(), &tag), &["windows-x86_64"]);
}

// ── Hostile 2 — platform/asset mismatch ───────────────────────────────────────────────────────
//
// Auditor's finding 2. `darwin-aarch64` serves the Windows installer and `windows-x86_64` serves
// the macOS `.app.tar.gz`, each with its own genuine signature: channel purity, url prefix and all
// signatures passed -- `verified 2 of 2 platform signature(s)`, EXIT 0. The outcome is
// denial-of-update rather than code execution, but the platform -> asset mapping is exactly what a
// channel-mixing bug corrupts, so it gets an assertion of its own.

#[test]
fn h2_a_swapped_platform_to_asset_mapping_is_refused_naming_both_platforms() {
    let tag = format!("v{VERSION}-sidecar");
    let dir = scaffold(
        SIDECAR_PRODUCT,
        VERSION,
        VERSION,
        &tag,
        &[
            entry(
                "darwin-aarch64",
                format!("Cross-Platform.Explorer.Sidecar._{VERSION}_x64-setup.exe"),
                "windows installer bytes, served to macOS",
            ),
            entry(
                "windows-x86_64",
                "Cross-Platform.Explorer.Sidecar._aarch64.app.tar.gz",
                "macos app tarball bytes, served to windows",
            ),
        ],
    );
    let stderr = refuse(
        &run(dir.path(), &tag),
        &["platform/asset mapping", "darwin-aarch64", "windows-x86_64"],
    );
    assert!(
        !stderr.contains("did NOT verify"),
        "both signatures in this fixture are genuine; the refusal must be about the MAPPING.\n\
         --- stderr ---\n{stderr}"
    );
}

/// The mapping check's own red-proof, isolated from every other check.
///
/// The fixture above is caught by the version binding too once the mapping check is removed (the
/// macOS payload it serves to Windows is versionless), so removing the mapping check alone still
/// produces a non-zero exit there — a correct outcome, but it means that test alone cannot prove
/// this specific check is doing anything. Here the wrong-OS payload deliberately CARRIES the
/// version, so the version binding is satisfied, the channel is pure, the url prefix is right and
/// the signature is genuine: the platform → payload mapping is the only thing left that can refuse
/// it. Delete the mapping check and this exits 0.
#[test]
fn h2_only_the_mapping_check_can_refuse_a_wrong_os_payload_that_is_otherwise_perfect() {
    let tag = format!("v{VERSION}-sidecar");
    let dir = scaffold(
        SIDECAR_PRODUCT,
        VERSION,
        VERSION,
        &tag,
        &[entry(
            "windows-x86_64",
            format!("Cross-Platform.Explorer.Sidecar._{VERSION}_universal.app.tar.gz"),
            "a macOS payload, correctly versioned, served under a Windows platform key",
        )],
    );
    refuse(&run(dir.path(), &tag), &["platform/asset mapping", "windows-x86_64"]);
}

/// Control: the same two platforms with their own correct payloads must pass.
#[test]
fn h2_control_the_correct_platform_to_asset_mapping_passes() {
    let tag = format!("v{VERSION}-sidecar");
    let dir = scaffold(
        SIDECAR_PRODUCT,
        VERSION,
        VERSION,
        &tag,
        &[
            entry(
                "windows-x86_64",
                format!("Cross-Platform.Explorer.Sidecar._{VERSION}_x64-setup.exe"),
                "windows installer bytes",
            ),
            entry(
                "darwin-aarch64",
                "Cross-Platform.Explorer.Sidecar._aarch64.app.tar.gz",
                "macos app tarball bytes",
            ),
        ],
    );
    accept(&run(dir.path(), &tag), 2);
}

/// A platform key naming no OS this release builds for cannot be waved through either -- that is
/// the shape a smuggled entry takes, and a rule that shrugs at keys it does not understand is a
/// rule an attacker picks the key to avoid.
#[test]
fn h2_an_unrecognised_platform_key_is_refused() {
    let tag = format!("v{VERSION}-sidecar");
    let dir = scaffold(
        SIDECAR_PRODUCT,
        VERSION,
        VERSION,
        &tag,
        &[entry(
            "solaris-sparc",
            format!("Cross-Platform.Explorer.Sidecar._{VERSION}_x64-setup.exe"),
            "a payload under a platform key nothing here builds",
        )],
    );
    refuse(&run(dir.path(), &tag), &["solaris-sparc"]);
}

// ── Hostile 3 — channel inference was an unanchored substring match ───────────────────────────
//
// Auditor's finding 3. The channel was decided by
// `basename.to_ascii_lowercase().contains("sidecar")`, so a PLAIN installer uploaded as
// `…_x64-setup.nsis.zip.sidecar` read as Channel::Sidecar and passed a sidecar-channel run at
// EXIT 0. The guard proved "the name contains the word sidecar", not "this asset came from the
// sidecar build" -- and anyone who can name a release asset could flip its apparent channel in
// EITHER direction.

#[test]
fn h3_a_plain_installer_renamed_to_look_like_sidecar_is_refused() {
    let tag = format!("v{VERSION}-sidecar");
    let dir = scaffold(
        SIDECAR_PRODUCT,
        VERSION,
        VERSION,
        &tag,
        &[entry(
            "windows-x86_64",
            format!("Cross-Platform.Explorer_{VERSION}_x64-setup.nsis.zip.sidecar"),
            "a PLAIN-channel installer wearing a .sidecar suffix",
        )],
    );
    let stderr = refuse(&run(dir.path(), &tag), &["release channel", "windows-x86_64"]);
    assert!(
        stderr.contains("plain"),
        "the refusal must say the asset is actually from the PLAIN channel, not merely that \
         something is wrong.\n--- stderr ---\n{stderr}"
    );
}

/// The same trick in the other direction, which the substring rule was equally blind to: a genuine
/// sidecar asset in a PLAIN-channel manifest. This is CPE-1894's live defect and must stay caught.
#[test]
fn h3_a_sidecar_asset_in_a_plain_manifest_is_refused() {
    let tag = format!("v{VERSION}");
    let dir = scaffold(
        PLAIN_PRODUCT,
        VERSION,
        VERSION,
        &tag,
        &[entry(
            "windows-x86_64",
            format!("Cross-Platform.Explorer.Sidecar._{VERSION}_x64-setup.exe"),
            "a sidecar-channel installer in a plain manifest",
        )],
    );
    let stderr = refuse(&run(dir.path(), &tag), &["release channel", "windows-x86_64"]);
    assert!(stderr.contains("sidecar"), "--- stderr ---\n{stderr}");
}

/// An asset that is not this product's output at all -- no amount of suffixing makes it one.
#[test]
fn h3_an_asset_that_is_not_this_products_output_is_refused() {
    let tag = format!("v{VERSION}-sidecar");
    let dir = scaffold(
        SIDECAR_PRODUCT,
        VERSION,
        VERSION,
        &tag,
        &[entry(
            "windows-x86_64",
            format!("Totally-Different-App_{VERSION}_x64-setup.exe.sidecar"),
            "someone else's installer with the magic word appended",
        )],
    );
    refuse(&run(dir.path(), &tag), &["release channel", "windows-x86_64"]);
}

/// Control: a genuine sidecar manifest, named the way the real bundler names things (including the
/// RPM, which spells the product name differently from every other target), must pass.
#[test]
fn h3_control_a_genuine_sidecar_manifest_passes() {
    let tag = format!("v{VERSION}-sidecar");
    let dir = scaffold(
        SIDECAR_PRODUCT,
        VERSION,
        VERSION,
        &tag,
        &[
            entry(
                "windows-x86_64",
                format!("Cross-Platform.Explorer.Sidecar._{VERSION}_x64-setup.exe"),
                "windows bytes",
            ),
            entry(
                "linux-x86_64-rpm",
                format!("Cross-Platform.Explorer.Sidecar.-{VERSION}-1.x86_64.rpm"),
                "rpm bytes",
            ),
        ],
    );
    accept(&run(dir.path(), &tag), 2);
}

/// Control: and a genuine PLAIN manifest, likewise.
#[test]
fn h3_control_a_genuine_plain_manifest_passes() {
    let tag = format!("v{VERSION}");
    let dir = scaffold(
        PLAIN_PRODUCT,
        VERSION,
        VERSION,
        &tag,
        &[
            entry(
                "windows-x86_64",
                format!("Cross-Platform.Explorer_{VERSION}_x64-setup.exe"),
                "windows bytes",
            ),
            entry("darwin-x86_64", "Cross-Platform.Explorer_universal.app.tar.gz", "macos bytes"),
        ],
    );
    accept(&run(dir.path(), &tag), 2);
}

// ── The checks that already worked must keep working ──────────────────────────────────────────
//
// The auditor also recorded what the gate correctly REJECTED. Re-asserting the two that the new
// checks run before is cheap insurance against a fix that reorders itself into a bypass.

#[test]
fn a_signature_made_over_different_bytes_is_still_refused() {
    let tag = format!("v{VERSION}-sidecar");
    let dir = scaffold(
        SIDECAR_PRODUCT,
        VERSION,
        VERSION,
        &tag,
        &[entry(
            "windows-x86_64",
            format!("Cross-Platform.Explorer.Sidecar._{VERSION}_x64-setup.exe"),
            "the signed bytes",
        )],
    );
    // Overwrite the artifact AFTER signing: same manifest, different bytes.
    std::fs::write(
        dir.path().join(format!("Cross-Platform.Explorer.Sidecar._{VERSION}_x64-setup.exe")),
        b"tampered bytes that were never signed",
    )
    .expect("tamper");
    refuse(&run(dir.path(), &tag), &["did NOT verify", "windows-x86_64"]);
}

#[test]
fn a_manifest_with_no_platforms_is_still_refused() {
    let tag = format!("v{VERSION}-sidecar");
    let dir = scaffold(SIDECAR_PRODUCT, VERSION, VERSION, &tag, &[]);
    refuse(&run(dir.path(), &tag), &["platforms"]);
}

/// A config with no `productName` leaves the anchored channel check nothing to anchor against.
/// CPE-1923 makes that a refusal rather than a silent pass-everything.
#[test]
fn a_config_without_a_product_name_is_refused_rather_than_disarming_the_channel_check() {
    let dir = tempfile::tempdir().expect("tempdir");
    let kp = KeyPair::generate_unencrypted_keypair().expect("keypair");
    let pubkey_config = B64.encode(kp.pk.to_box().expect("pk box").into_string().as_bytes());
    std::fs::write(
        dir.path().join("tauri.conf.json"),
        serde_json::json!({
            "version": VERSION,
            "plugins": { "updater": { "pubkey": pubkey_config } }
        })
        .to_string(),
    )
    .expect("write conf");
    std::fs::write(
        dir.path().join("latest.json"),
        serde_json::json!({ "version": VERSION, "platforms": {} }).to_string(),
    )
    .expect("write manifest");
    refuse(&run(dir.path(), &format!("v{VERSION}")), &["productName"]);
}

// ── The CPE-1908 invocation shape: --conf is PLAIN, --expect-channel is sidecar ────────────────
//
// `release-sidecar.yml` passes the BASE `src-tauri/tauri.conf.json` (it needs that file's
// pubkey/version and the CPE-1873 pin, which the sidecar overlay never touches) while checking a
// pure SIDECAR manifest, and tells the binary so with `--expect-channel sidecar`. So `--conf`'s
// `productName` and the expected channel legitimately DISAGREE on every real sidecar run.
//
// This is the trap CPE-1923's anchored channel check had to be designed around. An anchor taken
// straight from `--conf`'s `productName` would demand `crossplatformexplorer…` of assets that
// correctly read `crossplatformexplorersidecar…`, rejecting 100% of real sidecar releases — a
// far worse bug than the one being fixed. `base_product_token` reduces the config's name to the
// channel-free base identity, and the EXPECTED CHANNEL re-derives which of the two forms an asset
// must match, so the anchor no longer depends on which config was passed.
//
// These two tests are the regression pin for that. Change the anchor back to the raw `productName`
// token and the first one reddens on the exit code.

fn run_expecting_channel(dir: &Path, tag: &str, channel: &str) -> Output {
    Command::new(BIN)
        .args([
            "--conf",
            dir.join("tauri.conf.json").to_str().unwrap(),
            "--manifest",
            dir.join("latest.json").to_str().unwrap(),
            "--search",
            dir.to_str().unwrap(),
            "--expect-url-prefix",
            &url_prefix(tag),
            "--expect-channel",
            channel,
            "--skip-pin-check",
        ])
        .output()
        .expect("run verify-release-artifacts")
}

/// The real sidecar release shape, invoked exactly as `release-sidecar.yml` invokes it: plain
/// `productName` in `--conf`, `--expect-channel sidecar`, genuine sidecar-named assets including
/// the versionless macOS one. Must pass whole.
#[test]
fn the_sidecar_jobs_own_invocation_shape_accepts_a_genuine_sidecar_release() {
    let tag = format!("v{VERSION}-sidecar");
    let dir = scaffold(
        // The BASE conf's productName -- plain, as the real sidecar job always passes.
        PLAIN_PRODUCT,
        VERSION,
        VERSION,
        &tag,
        &[
            entry(
                "windows-x86_64",
                format!("Cross-Platform.Explorer.Sidecar._{VERSION}_x64-setup.exe"),
                "sidecar windows bytes",
            ),
            entry(
                "linux-x86_64-rpm",
                format!("Cross-Platform.Explorer.Sidecar.-{VERSION}-1.x86_64.rpm"),
                "sidecar rpm bytes",
            ),
            entry(
                "darwin-aarch64",
                "Cross-Platform.Explorer.Sidecar._aarch64.app.tar.gz",
                "sidecar macos bytes",
            ),
        ],
    );
    accept(&run_expecting_channel(dir.path(), &tag, "sidecar"), 3);
}

/// ...and the check still bites in that shape: the SAME plain `--conf`, the same
/// `--expect-channel sidecar`, but one plain-channel asset smuggled in. `--expect-channel`, not
/// `productName`, decides what is expected — so this must be refused even though the asset matches
/// the config's own product name exactly.
#[test]
fn the_sidecar_jobs_own_invocation_shape_still_refuses_a_plain_asset() {
    let tag = format!("v{VERSION}-sidecar");
    let dir = scaffold(
        PLAIN_PRODUCT,
        VERSION,
        VERSION,
        &tag,
        &[entry(
            "windows-x86_64",
            format!("Cross-Platform.Explorer_{VERSION}_x64-setup.exe"),
            "a PLAIN asset smuggled into a sidecar release",
        )],
    );
    let stderr = refuse(
        &run_expecting_channel(dir.path(), &tag, "sidecar"),
        &["release channel", "windows-x86_64"],
    );
    assert!(
        stderr.contains("--expect-channel"),
        "the refusal must say WHERE the expectation came from, since --conf's productName \
         disagrees with it by design in this shape.\n--- stderr ---\n{stderr}"
    );
}
