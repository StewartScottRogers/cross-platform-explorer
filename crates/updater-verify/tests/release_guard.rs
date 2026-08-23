//! Slice B end-to-end (CPE-1058): drive the `verify-release-artifacts` binary exactly as `release.yml`
//! does — a real `tauri.conf.json` (pubkey + version), a real `latest.json`, and a real artifact on disk,
//! all encoded in the same double-base64 shape Tauri uses — and assert the binary's exit status. Proves
//! the plumbing the unit tests can't reach: conf reading, artifact discovery by basename, and the exit code.

use std::process::Command;

use base64::Engine as _;
use minisign::KeyPair;

const B64: base64::engine::general_purpose::GeneralPurpose = base64::engine::general_purpose::STANDARD;
const BIN: &str = env!("CARGO_BIN_EXE_verify-release-artifacts");
const VERSION: &str = "1.2.3";
const ARTIFACT_NAME: &str = "app_1.2.3_x64-setup.nsis.zip";

/// Build a temp release tree and return its dir. `artifact_on_disk` are the bytes actually written to the
/// artifact file; the manifest signature is always computed over `signed_bytes`. Passing different values
/// simulates a tampered/corrupted artifact.
fn scaffold(signed_bytes: &[u8], artifact_on_disk: &[u8], manifest_version: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    let kp = KeyPair::generate_unencrypted_keypair().expect("keypair");
    let pubkey_config = B64.encode(kp.pk.to_box().expect("pk box").into_string().as_bytes());

    let sig = minisign::sign(
        Some(&kp.pk),
        &kp.sk,
        std::io::Cursor::new(signed_bytes),
        Some("trusted"),
        Some("untrusted"),
    )
    .expect("sign");
    let signature_field = B64.encode(sig.into_string().as_bytes());

    std::fs::write(root.join(ARTIFACT_NAME), artifact_on_disk).expect("write artifact");

    let manifest = serde_json::json!({
        "version": manifest_version,
        "platforms": {
            "windows-x86_64": {
                "signature": signature_field,
                "url": format!("https://example.com/releases/download/v{VERSION}/{ARTIFACT_NAME}")
            }
        }
    });
    std::fs::write(root.join("latest.json"), manifest.to_string()).expect("write manifest");

    let conf = serde_json::json!({
        "version": VERSION,
        "plugins": { "updater": { "pubkey": pubkey_config } }
    });
    std::fs::write(root.join("tauri.conf.json"), conf.to_string()).expect("write conf");

    dir
}

fn run(dir: &std::path::Path) -> std::process::Output {
    Command::new(BIN)
        .args([
            "--conf",
            dir.join("tauri.conf.json").to_str().unwrap(),
            "--search",
            dir.to_str().unwrap(),
        ])
        .output()
        .expect("run verify-release-artifacts")
}

/// Build a temp tree shaped like the real repo checkout `release.yml` runs the guard against:
/// `<root>/latest.json` (where tauri-action's `upload-version-json.ts` actually writes it —
/// `resolve(process.cwd(), 'latest.json')`, and `process.cwd()` is the job's un-overridden
/// working-directory, i.e. the repo root), `<root>/src-tauri/tauri.conf.json`, and the artifact under
/// `<root>/src-tauri/target/...` — the same relative shape as `--conf src-tauri/tauri.conf.json`
/// `--manifest latest.json` `--search src-tauri/target` run from the repo root (CPE-1872).
fn scaffold_repo_layout(signed_bytes: &[u8], artifact_on_disk: &[u8]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let src_tauri = root.join("src-tauri");
    let target = src_tauri.join("target").join("release").join("bundle").join("nsis");
    std::fs::create_dir_all(&target).expect("mkdir target tree");

    let kp = KeyPair::generate_unencrypted_keypair().expect("keypair");
    let pubkey_config = B64.encode(kp.pk.to_box().expect("pk box").into_string().as_bytes());
    let sig = minisign::sign(
        Some(&kp.pk),
        &kp.sk,
        std::io::Cursor::new(signed_bytes),
        Some("trusted"),
        Some("untrusted"),
    )
    .expect("sign");
    let signature_field = B64.encode(sig.into_string().as_bytes());

    std::fs::write(target.join(ARTIFACT_NAME), artifact_on_disk).expect("write artifact");

    let manifest = serde_json::json!({
        "version": VERSION,
        "platforms": {
            "windows-x86_64": {
                "signature": signature_field,
                "url": format!("https://example.com/releases/download/v{VERSION}/{ARTIFACT_NAME}")
            }
        }
    });
    // The load-bearing bit: latest.json sits at the repo root, a sibling of src-tauri/, NOT anywhere
    // under src-tauri/target — that mismatch is exactly what made every run of the real workflow fail.
    std::fs::write(root.join("latest.json"), manifest.to_string()).expect("write manifest");

    let conf = serde_json::json!({
        "version": VERSION,
        "plugins": { "updater": { "pubkey": pubkey_config } }
    });
    std::fs::write(src_tauri.join("tauri.conf.json"), conf.to_string()).expect("write conf");

    dir
}

/// Runs the binary exactly the way `release.yml` invokes it post-CPE-1872: from the repo root, with
/// `--manifest` pointed at the known tauri-action write location instead of relying on `--search`
/// discovery to stumble onto it.
fn run_repo_layout(root: &std::path::Path) -> std::process::Output {
    Command::new(BIN)
        .current_dir(root)
        .args([
            "--conf",
            "src-tauri/tauri.conf.json",
            "--manifest",
            "latest.json",
            "--search",
            "src-tauri/target",
        ])
        .output()
        .expect("run verify-release-artifacts")
}

#[test]
fn valid_release_passes() {
    let bytes = b"the real installer bytes";
    let dir = scaffold(bytes, bytes, VERSION);
    let out = run(dir.path());
    assert!(
        out.status.success(),
        "expected success; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("1 platform signature(s) verified"));
}

#[test]
fn tampered_artifact_fails_the_release() {
    let dir = scaffold(b"the real installer bytes", b"CORRUPTED installer bytes", VERSION);
    let out = run(dir.path());
    assert!(!out.status.success(), "tampered artifact must fail the guard");
    assert!(String::from_utf8_lossy(&out.stderr).contains("did NOT verify"));
}

#[test]
fn version_mismatch_fails_the_release() {
    let bytes = b"installer";
    // Manifest claims a different version than tauri.conf.json.
    let dir = scaffold(bytes, bytes, "9.9.9");
    let out = run(dir.path());
    assert!(!out.status.success(), "version mismatch must fail the guard");
    assert!(String::from_utf8_lossy(&out.stderr).contains("does not match expected"));
}

/// CPE-1872 (RED, reproducing the real bug): `latest.json` sits where tauri-action actually writes it —
/// the repo root, a sibling of `src-tauri/` — while the old workflow's `--search src-tauri/target`
/// (with no `--manifest`) only ever discovers files *under* `src-tauri/target`. This must fail exactly
/// the way every real run of `release.yml` failed from 2026-08-04 to 2026-08-23: "no latest.json found".
#[test]
fn manifest_at_repo_root_is_not_found_by_search_under_target_alone() {
    let bytes = b"the real installer bytes";
    let dir = scaffold_repo_layout(bytes, bytes);
    let out = Command::new(BIN)
        .current_dir(dir.path())
        .args(["--conf", "src-tauri/tauri.conf.json", "--search", "src-tauri/target"])
        .output()
        .expect("run verify-release-artifacts");
    assert!(
        !out.status.success(),
        "a manifest outside the search dir must be a hard failure, not a silent pass"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("no latest.json found"));
}

/// CPE-1872 (GREEN, the fix): same repo-root layout, but invoked the way the fixed `release.yml` now
/// invokes it — `--manifest latest.json` pointed straight at tauri-action's actual write location, run
/// from the repo root. The signature must verify over the real artifact bytes under `src-tauri/target`.
#[test]
fn manifest_at_repo_root_is_found_and_verified_via_explicit_manifest_flag() {
    let bytes = b"the real installer bytes";
    let dir = scaffold_repo_layout(bytes, bytes);
    let out = run_repo_layout(dir.path());
    assert!(
        out.status.success(),
        "expected success; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("1 platform signature(s) verified"));
}

/// CPE-1872 (RED): the fixed invocation still fails hard — never skips — when the manifest tauri-action
/// was supposed to write simply isn't there (e.g. `includeUpdaterJson` didn't run, or a future refactor
/// moves the write location again). A missing manifest must never read as "nothing to verify, pass".
#[test]
fn missing_manifest_at_expected_location_is_a_hard_failure() {
    let bytes = b"the real installer bytes";
    let dir = scaffold_repo_layout(bytes, bytes);
    std::fs::remove_file(dir.path().join("latest.json")).expect("remove manifest");
    let out = run_repo_layout(dir.path());
    assert!(!out.status.success(), "a missing manifest must fail the guard, never skip it");
    assert!(String::from_utf8_lossy(&out.stderr).contains("cannot read"));
}

/// CPE-1872 (RED): a corrupted signature on the manifest tauri-action actually wrote at the real
/// location must still be caught — the location fix must not have weakened the crypto check.
#[test]
fn tampered_artifact_at_repo_root_layout_still_fails_the_release() {
    let dir = scaffold_repo_layout(b"the real installer bytes", b"CORRUPTED installer bytes");
    let out = run_repo_layout(dir.path());
    assert!(!out.status.success(), "tampered artifact must fail the guard");
    assert!(String::from_utf8_lossy(&out.stderr).contains("did NOT verify"));
}
