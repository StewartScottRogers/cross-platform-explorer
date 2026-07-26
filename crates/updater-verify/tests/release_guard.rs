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
