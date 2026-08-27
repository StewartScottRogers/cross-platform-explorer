//! Slice B end-to-end (CPE-1058): drive the `verify-release-artifacts` binary over realistic inputs —
//! a real `tauri.conf.json` (pubkey + version), a real `latest.json`, and a real artifact on disk,
//! all encoded in the same double-base64 shape Tauri uses — and assert the binary's exit status. Proves
//! the plumbing the unit tests can't reach: conf reading, artifact discovery by basename, and the exit code.
//!
//! CPE-1917: this file's argv is written by hand and does NOT track `release.yml`'s (it used to claim
//! it did; see `run_repo_layout`). The workflow's own invocation — read out of the YAML and executed
//! against a tree laid out by the workflow's own download step — is pinned in
//! `tests/release_workflow_wiring.rs`. Keep the two concerns separate: this file is about what the
//! binary does, that one is about what the release pipeline asks it to do.

use std::process::Command;

use base64::Engine as _;
use minisign::KeyPair;

const B64: base64::engine::general_purpose::GeneralPurpose = base64::engine::general_purpose::STANDARD;
const BIN: &str = env!("CARGO_BIN_EXE_verify-release-artifacts");
const VERSION: &str = "1.2.3";
/// CPE-1923: fixture asset names are now the shape the real bundler emits -- anchored to the
/// product name and carrying the version -- because the guard now binds both. `app_1.2.3_…`
/// no longer resembles anything this release process could have produced, which is the point.
const ARTIFACT_NAME: &str = "Cross-Platform.Explorer_1.2.3_x64-setup.exe";
/// The plain channel's `productName`, required by the anchored channel check (CPE-1923).
const PRODUCT_NAME: &str = "Cross-Platform Explorer";

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
        "productName": PRODUCT_NAME,
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
            // CPE-1873: these fixtures use a fresh, throwaway keypair per test, unrelated to the
            // repo's real pinned pubkey -- they exercise manifest/signature logic, not the pin.
            "--skip-pin-check",
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
///
/// CPE-1917: that argv is CPE-1872's round-1 shape, not what `release.yml` runs today — see
/// `run_repo_layout`'s own comment, and `tests/release_workflow_wiring.rs` for the argv the workflow
/// actually uses, read from the YAML rather than restated here.
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
        "productName": PRODUCT_NAME,
        "plugins": { "updater": { "pubkey": pubkey_config } }
    });
    std::fs::write(src_tauri.join("tauri.conf.json"), conf.to_string()).expect("write conf");

    dir
}

/// Runs the binary from the repo root with `--manifest` pointed at tauri-action's known write
/// location instead of relying on `--search` discovery to stumble onto it.
///
/// CPE-1917, correcting this comment: it used to claim this was "exactly the way `release.yml`
/// invokes it post-CPE-1872". That was true for one commit. CPE-1872 round 2 moved the check out of
/// the matrix into the post-matrix `verify-published-manifest` job, which downloads the PUBLISHED
/// manifest and its assets and runs `--manifest release-assets/latest.json --search release-assets`
/// — and this hard-coded argv was never updated, so a test advertising itself as the workflow's
/// mirror had quietly stopped tracking the workflow at all. What it still legitimately proves is the
/// *binary's* behaviour when the manifest lives outside every `--search` dir, which is worth keeping
/// on its own terms. The workflow's real argv is pinned — read out of `release.yml` and executed —
/// in `tests/release_workflow_wiring.rs`.
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
            // CPE-1873: throwaway per-test keypair, not the repo's real pin -- see the comment on `run()`.
            "--skip-pin-check",
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
    assert!(String::from_utf8_lossy(&out.stdout).contains("verified 1 of 1 platform signature(s)"));
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
        .args(["--conf", "src-tauri/tauri.conf.json", "--search", "src-tauri/target", "--skip-pin-check"])
        .output()
        .expect("run verify-release-artifacts");
    assert!(
        !out.status.success(),
        "a manifest outside the search dir must be a hard failure, not a silent pass"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("no latest.json found"));
}

/// CPE-1872 (GREEN, round 1's fix): same repo-root layout, but with `--manifest latest.json` pointed
/// straight at tauri-action's actual write location, run from the repo root. The signature must verify
/// over the real artifact bytes under `src-tauri/target`.
///
/// CPE-1917 round 2 (Reviewer): this used to say "invoked the way the fixed `release.yml` now invokes
/// it". It is not — today's invocation is `--manifest release-assets/latest.json --search
/// release-assets` over assets downloaded from the draft release, and has been since CPE-1872 round 2
/// moved the check into `verify-published-manifest`. Same species of stale provenance claim corrected
/// elsewhere in this file; the workflow's real argv is read from the YAML and executed in
/// `tests/release_workflow_wiring.rs`. What this test still proves on its own terms is that an
/// explicit `--manifest` outside every `--search` dir is found and verified.
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
    assert!(String::from_utf8_lossy(&out.stdout).contains("verified 1 of 1 platform signature(s)"));
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

// -- CPE-1872 round 2: independent security-audit findings ----------------------------------------
//
// The auditor built real minisign keypairs + eleven fixture releases against the round-1 binary and
// found two holes in how it decided what counted as "verified". Both reproduced independently on this
// machine against the pre-fix binary before being fixed:
//
//   RED  smuggled_extra_platform: exit=0 -- an honest windows entry + a linux-x86_64 entry pointing
//        at https://evil.example/pwn.AppImage.tar.gz, signed by a DIFFERENT keypair, passed because
//        no local artifact existed for the smuggled platform, so its crypto check was silently SKIPPED
//        rather than failed.
//   RED  basename_decoy: exit=0 -- a same-named file elsewhere in the search tree (readdir visits it
//        first) held the bytes the signature actually verifies against, while the genuine build-output
//        file of the identical basename was shadowed and never read.
//
// Both are now GREEN (non-zero exit) below: lib.rs's verify_update_manifest treats a platform with no
// locally-fetchable artifact as ArtifactUnavailable (a hard failure, never a skip), and
// verify-release-artifacts.rs now hard-fails the moment ANY basename is indexed more than once under
// the search dirs, rather than silently keeping whichever file a directory walk happened to visit first.

fn sign_bytes(keypair: &minisign::KeyPair, bytes: &[u8]) -> String {
    let sig = minisign::sign(
        Some(&keypair.pk),
        &keypair.sk,
        std::io::Cursor::new(bytes),
        Some("trusted"),
        Some("untrusted"),
    )
    .expect("sign");
    B64.encode(sig.into_string().as_bytes())
}

fn pubkey_config_field(keypair: &minisign::KeyPair) -> String {
    B64.encode(keypair.pk.to_box().expect("pk box").into_string().as_bytes())
}

/// CPE-1872 Finding 1 (HIGH, auditor's smuggled_extra_platform): a manifest carries an honest,
/// correctly-signed windows-x86_64 entry alongside a linux-x86_64 entry whose URL points at an asset
/// that is never built/served locally, signed by a key that is NOT the one configured in
/// tauri.conf.json. Before the fix this was EXIT=0: the windows platform verified, the linux platform's
/// crypto check was silently skipped (no local artifact to check it against), and "at least one
/// signature checked" was treated as good enough. Must now fail.
#[test]
fn smuggled_extra_platform_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let honest = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
    let evil = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");

    let win_name = "Cross-Platform.Explorer_1.2.3_x64-setup.exe";
    let win_bytes = b"the real windows installer bytes";
    std::fs::write(root.join(win_name), win_bytes).expect("write windows artifact");
    let win_sig = sign_bytes(&honest, win_bytes);

    // Attacker's entry: no local artifact will ever exist for this basename, and it's signed by a key
    // that isn't the configured pubkey -- both facts a real verifier must catch.
    let evil_sig = sign_bytes(&evil, b"whatever the attacker wants to ship");

    let manifest = serde_json::json!({
        "version": VERSION,
        "platforms": {
            "windows-x86_64": {
                "signature": win_sig,
                "url": format!("https://example.com/releases/download/v{VERSION}/{win_name}")
            },
            "linux-x86_64": {
                "signature": evil_sig,
                "url": "https://evil.example/Cross-Platform.Explorer_1.2.3_amd64.AppImage"
            }
        }
    });
    std::fs::write(root.join("latest.json"), manifest.to_string()).expect("write manifest");
    let conf = serde_json::json!({
        "version": VERSION,
        "productName": PRODUCT_NAME,
        "plugins": { "updater": { "pubkey": pubkey_config_field(&honest) } }
    });
    std::fs::write(root.join("tauri.conf.json"), conf.to_string()).expect("write conf");

    let out = Command::new(BIN)
        .args(["--conf", root.join("tauri.conf.json").to_str().unwrap(), "--search", root.to_str().unwrap(), "--skip-pin-check"])
        .output()
        .expect("run verify-release-artifacts");
    assert!(
        !out.status.success(),
        "a smuggled platform entry with no locally-checkable artifact must fail the guard, not pass \
         on the strength of an unrelated platform's signature; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("linux-x86_64"));
}

/// CPE-1872 Finding 1 control (the auditor's smuggled_local_name): identical to the scenario above,
/// except the smuggled basename DOES exist locally. This must fail too, and for a DIFFERENT reason
/// (signature verification, not artifact availability) -- proving the fix above closes the "missing
/// artifact" hole specifically, without the crypto check itself having been broken all along.
#[test]
fn smuggled_local_name_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let honest = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
    let evil = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");

    let win_name = "Cross-Platform.Explorer_1.2.3_x64-setup.exe";
    let win_bytes = b"the real windows installer bytes";
    std::fs::write(root.join(win_name), win_bytes).expect("write windows artifact");
    let win_sig = sign_bytes(&honest, win_bytes);

    let evil_name = "Cross-Platform.Explorer_1.2.3_amd64.deb";
    let evil_bytes = b"whatever the attacker wants to ship";
    std::fs::write(root.join(evil_name), evil_bytes).expect("write smuggled artifact locally");
    let evil_sig = sign_bytes(&evil, evil_bytes);

    let manifest = serde_json::json!({
        "version": VERSION,
        "platforms": {
            "windows-x86_64": {
                "signature": win_sig,
                "url": format!("https://example.com/releases/download/v{VERSION}/{win_name}")
            },
            "linux-x86_64": {
                "signature": evil_sig,
                "url": format!("https://evil.example/{evil_name}")
            }
        }
    });
    std::fs::write(root.join("latest.json"), manifest.to_string()).expect("write manifest");
    let conf = serde_json::json!({
        "version": VERSION,
        "productName": PRODUCT_NAME,
        "plugins": { "updater": { "pubkey": pubkey_config_field(&honest) } }
    });
    std::fs::write(root.join("tauri.conf.json"), conf.to_string()).expect("write conf");

    let out = Command::new(BIN)
        .args(["--conf", root.join("tauri.conf.json").to_str().unwrap(), "--search", root.to_str().unwrap(), "--skip-pin-check"])
        .output()
        .expect("run verify-release-artifacts");
    assert!(!out.status.success(), "signature from the wrong key must fail, artifact-availability aside");
    assert!(String::from_utf8_lossy(&out.stderr).contains("did NOT verify"));
}

/// CPE-1872 Finding 2 (MEDIUM, auditor's basename_decoy): two files share a basename somewhere under
/// the search tree -- one (in a directory that a directory walk visits first) holds the bytes the
/// signature actually verifies against, the other (the genuine build-output location) holds different
/// bytes. Before the fix, first-wins indexing meant the decoy could shadow the real build output and
/// pass EXIT=0 while the real file was never read. Must now hard-fail as ambiguous, regardless of which
/// file "would have" won the race.
#[test]
fn basename_decoy_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let kp = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");

    let name = "Cross-Platform.Explorer_1.2.3_x64-setup.exe";
    let decoy_bytes = b"bytes the signature verifies against";
    let real_build_output_bytes = b"a DIFFERENT current build's real output";
    let decoy_dir = root.join("aaa_decoy");
    let real_dir = root.join("real");
    std::fs::create_dir_all(&decoy_dir).expect("mkdir decoy");
    std::fs::create_dir_all(&real_dir).expect("mkdir real");
    std::fs::write(decoy_dir.join(name), decoy_bytes).expect("write decoy");
    std::fs::write(real_dir.join(name), real_build_output_bytes).expect("write real build output");

    let sig = sign_bytes(&kp, decoy_bytes);
    let manifest = serde_json::json!({
        "version": VERSION,
        "platforms": {
            "windows-x86_64": {
                "signature": sig,
                "url": format!("https://example.com/releases/download/v{VERSION}/{name}")
            }
        }
    });
    std::fs::write(root.join("latest.json"), manifest.to_string()).expect("write manifest");
    let conf = serde_json::json!({
        "version": VERSION,
        "productName": PRODUCT_NAME,
        "plugins": { "updater": { "pubkey": pubkey_config_field(&kp) } }
    });
    std::fs::write(root.join("tauri.conf.json"), conf.to_string()).expect("write conf");

    let out = Command::new(BIN)
        .args(["--conf", root.join("tauri.conf.json").to_str().unwrap(), "--search", root.to_str().unwrap(), "--skip-pin-check"])
        .output()
        .expect("run verify-release-artifacts");
    assert!(
        !out.status.success(),
        "a basename appearing more than once under the search tree must be refused as ambiguous, \
         never silently resolved by directory-walk order; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("ambiguous"));
}

// -- CPE-1872 round 3: Finding B (the url-binding gap the round-2 redesign left open) --------------
//
// The download step resolves every platform `url` to its LAST path segment and fetches that basename
// from OUR release; the verifier matches a `url` back to a local file the same way. Neither step ever
// looks at the url's host or the path segments before the basename -- so a manifest can carry a
// perfectly genuine, correctly-signed artifact under a url that points real updater clients at a
// foreign host, or at the right repo but the wrong tag, and this pipeline verified it clean. The
// auditor's `n1_foreign_host_same_basename` / `n2_wrong_tag_same_basename` fixtures below reproduce
// that exactly (EXIT=0 without `--expect-url-prefix`) and prove `--expect-url-prefix` -- now always
// passed by release.yml's verify step -- closes it (EXIT!=0).

/// Same shape as `scaffold`, but the manifest platform's `url` is caller-supplied instead of the
/// hard-coded example.com one -- lets a fixture put a genuine, correctly-signed artifact behind an
/// arbitrary url (foreign host / wrong tag / anything else) while the LOCAL file is still indexed and
/// read by its basename, exactly like the real download step would leave it.
fn scaffold_with_url(signed_bytes: &[u8], url: &str) -> tempfile::TempDir {
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

    std::fs::write(root.join(ARTIFACT_NAME), signed_bytes).expect("write artifact");

    let manifest = serde_json::json!({
        "version": VERSION,
        "platforms": {
            "windows-x86_64": { "signature": signature_field, "url": url }
        }
    });
    std::fs::write(root.join("latest.json"), manifest.to_string()).expect("write manifest");

    let conf = serde_json::json!({
        "version": VERSION,
        "productName": PRODUCT_NAME,
        "plugins": { "updater": { "pubkey": pubkey_config } }
    });
    std::fs::write(root.join("tauri.conf.json"), conf.to_string()).expect("write conf");

    dir
}

fn run_with_optional_url_prefix(dir: &std::path::Path, prefix: Option<&str>) -> std::process::Output {
    let mut cmd = Command::new(BIN);
    cmd.args(["--conf", dir.join("tauri.conf.json").to_str().unwrap(), "--search", dir.to_str().unwrap(), "--skip-pin-check"]);
    if let Some(p) = prefix {
        cmd.args(["--expect-url-prefix", p]);
    }
    cmd.output().expect("run verify-release-artifacts")
}

fn real_release_url_prefix() -> String {
    format!("https://github.com/StewartScottRogers/cross-platform-explorer/releases/download/v{VERSION}/")
}

/// CPE-1872 Finding B, auditor's `n1_foreign_host_same_basename`, RED half: without
/// `--expect-url-prefix` (the shape of every invocation before this fix), a foreign host serving the
/// identical basename to a genuine, correctly-signed artifact passes clean. Documents that the gap is
/// real and that the crypto check alone can never close it -- the artifact bytes genuinely are what was
/// signed, the url just lies about where to get them.
#[test]
fn n1_foreign_host_same_basename_passes_without_url_prefix_check() {
    let bytes = b"the real installer bytes";
    let evil_url = format!("https://evil.example/pwn/{ARTIFACT_NAME}");
    let dir = scaffold_with_url(bytes, &evil_url);
    let out = run_with_optional_url_prefix(dir.path(), None);
    assert!(
        out.status.success(),
        "documents the pre-fix gap: without --expect-url-prefix a foreign host must still pass here; \
         stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// CPE-1872 Finding B, `n1_foreign_host_same_basename`, GREEN half: the exact same fixture, but invoked
/// the way release.yml's verify step now always invokes it -- with `--expect-url-prefix` set to this
/// repo's real release-download prefix. Must now fail.
#[test]
fn n1_foreign_host_same_basename_is_rejected_with_url_prefix_check() {
    let bytes = b"the real installer bytes";
    let evil_url = format!("https://evil.example/pwn/{ARTIFACT_NAME}");
    let dir = scaffold_with_url(bytes, &evil_url);
    let prefix = real_release_url_prefix();
    let out = run_with_optional_url_prefix(dir.path(), Some(&prefix));
    assert!(
        !out.status.success(),
        "a foreign-host url must fail once --expect-url-prefix is enforced; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("do not start with the expected prefix"));
}

/// CPE-1872 Finding B, auditor's `n2_wrong_tag_same_basename`, RED half: right host and repo, WRONG
/// release tag, same basename as a genuine artifact. Same class of gap as n1 -- must pass pre-fix.
#[test]
fn n2_wrong_tag_same_basename_passes_without_url_prefix_check() {
    let bytes = b"the real installer bytes";
    let wrong_tag_url = format!(
        "https://github.com/StewartScottRogers/cross-platform-explorer/releases/download/v0.0.1/{ARTIFACT_NAME}"
    );
    let dir = scaffold_with_url(bytes, &wrong_tag_url);
    let out = run_with_optional_url_prefix(dir.path(), None);
    assert!(
        out.status.success(),
        "documents the pre-fix gap: without --expect-url-prefix a wrong-tag url must still pass here; \
         stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// CPE-1872 Finding B, `n2_wrong_tag_same_basename`, GREEN half: must now fail once
/// `--expect-url-prefix` (release.yml's real prefix) is enforced.
#[test]
fn n2_wrong_tag_same_basename_is_rejected_with_url_prefix_check() {
    let bytes = b"the real installer bytes";
    let wrong_tag_url = format!(
        "https://github.com/StewartScottRogers/cross-platform-explorer/releases/download/v0.0.1/{ARTIFACT_NAME}"
    );
    let dir = scaffold_with_url(bytes, &wrong_tag_url);
    let prefix = real_release_url_prefix();
    let out = run_with_optional_url_prefix(dir.path(), Some(&prefix));
    assert!(
        !out.status.success(),
        "a wrong-tag url must fail once --expect-url-prefix is enforced; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("do not start with the expected prefix"));
}

/// Control: a genuine url that DOES match the expected prefix must still pass once
/// `--expect-url-prefix` is enforced -- proves the check doesn't just fail everything.
#[test]
fn matching_prefix_url_still_passes_with_url_prefix_check_enforced() {
    let bytes = b"the real installer bytes";
    let good_url = format!("{}{ARTIFACT_NAME}", real_release_url_prefix());
    let dir = scaffold_with_url(bytes, &good_url);
    let prefix = real_release_url_prefix();
    let out = run_with_optional_url_prefix(dir.path(), Some(&prefix));
    assert!(
        out.status.success(),
        "a genuine matching-prefix url must still pass with the check enforced; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

// -- CPE-1894: channel-purity guard, end-to-end through the real binary ---------------------------
//
// release.yml's `v*` tag trigger used to match `-sidecar` tags too, so the plain-build workflow fired
// on a sidecar tag push and merged its plain installers into the SAME draft release the sidecar
// workflow was populating -- one manifest naming assets from two different products. `--conf` here is
// always `src-tauri/tauri.conf.json`, the plain config, so a platform asset whose basename carries
// "sidecar" is exactly that defect recurring and must fail the guard by name.

/// Like `scaffold_with_url`, but also lets the caller set `tauri.conf.json`'s `productName` -- needed
/// to exercise the channel-purity check, which reads it to decide the manifest's expected channel.
fn scaffold_with_url_and_product_name(signed_bytes: &[u8], url: &str, product_name: &str) -> tempfile::TempDir {
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

    // The local artifact file must be named after the url's OWN basename (matching by basename is
    // exactly how the real download/verify pipeline works, per `scaffold_with_url` above) -- not the
    // fixed `ARTIFACT_NAME`, since these tests deliberately vary the basename to carry (or not carry)
    // the sidecar product token the channel check keys on.
    let basename = url.rsplit('/').next().expect("url has a basename");
    std::fs::write(root.join(basename), signed_bytes).expect("write artifact");

    let manifest = serde_json::json!({
        "version": VERSION,
        "platforms": {
            "windows-x86_64": { "signature": signature_field, "url": url }
        }
    });
    std::fs::write(root.join("latest.json"), manifest.to_string()).expect("write manifest");

    let conf = serde_json::json!({
        "version": VERSION,
        "productName": product_name,
        "plugins": { "updater": { "pubkey": pubkey_config } }
    });
    std::fs::write(root.join("tauri.conf.json"), conf.to_string()).expect("write conf");

    dir
}

/// RED, reproducing the live bug at the binary level: `--conf` declares the plain product name (as
/// `release.yml` always does), but the one platform's asset url names a sidecar-built installer. This
/// must fail, and the failure text must name the offending platform.
#[test]
fn a_sidecar_asset_in_a_plain_manifest_is_rejected_by_name() {
    let bytes = b"the real installer bytes";
    let sidecar_url = format!(
        "https://example.com/releases/download/v{VERSION}/Cross-Platform.Explorer_(Sidecar)_{VERSION}_x64-setup.nsis.zip"
    );
    let dir = scaffold_with_url_and_product_name(bytes, &sidecar_url, "Cross-Platform Explorer");
    let out = run(dir.path());
    assert!(
        !out.status.success(),
        "a sidecar-channel asset in a plain-channel manifest must fail the guard; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    // CPE-1923 re-worded this refusal so every failure names WHICH property failed.
    assert!(stderr.contains("PROPERTY FAILED -- release channel"), "stderr={stderr}");
    assert!(stderr.contains("windows-x86_64"), "must name the offending platform; stderr={stderr}");
}

/// Control: the SAME sidecar-named asset, but `--conf` now declares the sidecar product name too --
/// channel-consistent, so this check must pass (the crypto/version checks below it still run as usual).
#[test]
fn a_sidecar_asset_in_a_sidecar_manifest_passes_the_channel_check() {
    let bytes = b"the real installer bytes";
    let sidecar_url = format!(
        "https://example.com/releases/download/v{VERSION}/Cross-Platform.Explorer_(Sidecar)_{VERSION}_x64-setup.nsis.zip"
    );
    let dir = scaffold_with_url_and_product_name(bytes, &sidecar_url, "Cross-Platform Explorer (Sidecar)");
    let out = run(dir.path());
    assert!(
        out.status.success(),
        "a channel-consistent sidecar manifest must pass; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Control: an ordinary plain asset under the ordinary plain product name -- the common case -- must
/// keep passing. Guards against the channel check breaking normal releases.
#[test]
fn a_plain_asset_in_a_plain_manifest_passes_the_channel_check() {
    let bytes = b"the real installer bytes";
    let plain_url = format!("https://example.com/releases/download/v{VERSION}/{ARTIFACT_NAME}");
    let dir = scaffold_with_url_and_product_name(bytes, &plain_url, "Cross-Platform Explorer");
    let out = run(dir.path());
    assert!(
        out.status.success(),
        "a channel-consistent plain manifest must pass; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

// -- CPE-1908: the guard now actually runs against the SIDECAR channel too -------------------------
//
// `release-sidecar.yml`'s `--conf` is the BASE `src-tauri/tauri.conf.json` (needed for pubkey/version/
// the CPE-1873 pin, none of which the sidecar overlay touches) -- so its `productName` is always the
// PLAIN one, "Cross-Platform Explorer". Deriving the expected channel from that conf's productName
// (like the plain job does) would therefore always resolve to Plain even when checking the sidecar
// channel's own manifest -- exactly the gap this ticket exists to close. `--expect-channel sidecar` is
// how the sidecar job tells the binary which channel it's actually checking, independent of `--conf`'s
// productName. These fixtures build the conf with the ordinary PLAIN productName throughout (matching
// the real base tauri.conf.json byte-for-byte in shape) and vary only `--expect-channel` and the
// manifest's own asset names -- proving the flag, not the productName, decides the expected channel.

/// Two artifacts on disk with different basenames, and a manifest whose two platforms point at them by
/// url. Lets a single fixture carry both a sidecar-named and a plain-named asset in one manifest, which
/// `scaffold_with_url_and_product_name` (single-platform) can't express.
fn scaffold_mixed_manifest(product_name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let kp = KeyPair::generate_unencrypted_keypair().expect("keypair");
    let pubkey_config = B64.encode(kp.pk.to_box().expect("pk box").into_string().as_bytes());

    let sidecar_name = format!("Cross-Platform.Explorer_(Sidecar)_{VERSION}_x64-setup.nsis.zip");
    let sidecar_url = format!("https://example.com/releases/download/v{VERSION}-sidecar/{sidecar_name}");
    let sidecar_bytes = b"sidecar windows installer bytes";
    std::fs::write(root.join(&sidecar_name), sidecar_bytes).expect("write sidecar artifact");
    let sidecar_sig = sign_bytes(&kp, sidecar_bytes);

    let plain_name = ARTIFACT_NAME.to_string();
    let plain_url = format!("https://example.com/releases/download/v{VERSION}-sidecar/{plain_name}");
    let plain_bytes = b"a PLAIN-channel artifact smuggled into the sidecar manifest";
    std::fs::write(root.join(&plain_name), plain_bytes).expect("write plain artifact");
    let plain_sig = sign_bytes(&kp, plain_bytes);

    let manifest = serde_json::json!({
        "version": VERSION,
        "platforms": {
            "windows-x86_64": { "signature": sidecar_sig, "url": sidecar_url },
            "linux-x86_64": { "signature": plain_sig, "url": plain_url },
        }
    });
    std::fs::write(root.join("latest.json"), manifest.to_string()).expect("write manifest");

    let conf = serde_json::json!({
        "version": VERSION,
        "productName": product_name,
        "plugins": { "updater": { "pubkey": pubkey_config } }
    });
    std::fs::write(root.join("tauri.conf.json"), conf.to_string()).expect("write conf");

    dir
}

fn run_with_expect_channel(dir: &std::path::Path, channel: &str) -> std::process::Output {
    Command::new(BIN)
        .args([
            "--conf",
            dir.join("tauri.conf.json").to_str().unwrap(),
            "--search",
            dir.to_str().unwrap(),
            "--expect-channel",
            channel,
            "--skip-pin-check",
        ])
        .output()
        .expect("run verify-release-artifacts")
}

/// RED: this reproduces exactly what `release-sidecar.yml`'s job checks for -- a manifest that is
/// SUPPOSED to be sidecar-pure (dispatched under the sidecar tag) but carries one plain-channel asset,
/// checked with `--conf` pointed at the ordinary base conf (plain productName, as the real sidecar job's
/// `--conf` always is) plus `--expect-channel sidecar`. Must fail and name the offending platform, same
/// as CPE-1894's plain-side red-proof did for the mirror-image case.
#[test]
fn a_plain_asset_in_a_manifest_expected_sidecar_is_rejected_by_name() {
    let dir = scaffold_mixed_manifest("Cross-Platform Explorer"); // base conf's real productName
    let out = run_with_expect_channel(dir.path(), "sidecar");
    assert!(
        !out.status.success(),
        "a plain-channel asset in a manifest expected to be sidecar-pure must fail the guard; \
         stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    // CPE-1923 re-worded this refusal so every failure names WHICH property failed, and prints
    // offenders as `platform: reason` rather than `platform -> channel`.
    assert!(stderr.contains("PROPERTY FAILED -- release channel"), "stderr={stderr}");
    assert!(stderr.contains("linux-x86_64"), "must name the offending platform; stderr={stderr}");
    // And must NOT name the honest sidecar platform as an offender too.
    assert!(!stderr.contains("windows-x86_64:"), "must not falsely flag the honest platform; stderr={stderr}");
}

/// Control proving the flag actually flips the expectation (not just always failing a mixed manifest):
/// the IDENTICAL mixed fixture, checked with `--expect-channel plain` instead, must fail in the MIRROR
/// direction -- naming the sidecar platform, not the plain one.
#[test]
fn the_same_mixed_manifest_checked_as_expected_plain_names_the_other_platform() {
    let dir = scaffold_mixed_manifest("Cross-Platform Explorer");
    let out = run_with_expect_channel(dir.path(), "plain");
    assert!(!out.status.success(), "a mixed manifest must fail against either expectation");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("windows-x86_64"), "must name the sidecar platform when expecting plain; stderr={stderr}");
    assert!(!stderr.contains("linux-x86_64:"), "must not flag the honest plain platform; stderr={stderr}");
}

/// GREEN: the fix in its real shape -- a UNIFORM sidecar manifest (every asset sidecar-named), `--conf`
/// still the base plain-productName conf (exactly as `release-sidecar.yml` invokes it), `--expect-channel
/// sidecar`. This is what a healthy sidecar release checks clean against.
#[test]
fn a_uniform_sidecar_manifest_passes_with_expect_channel_sidecar() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let kp = KeyPair::generate_unencrypted_keypair().expect("keypair");
    let pubkey_config = B64.encode(kp.pk.to_box().expect("pk box").into_string().as_bytes());

    let names = [
        format!("Cross-Platform.Explorer_(Sidecar)_{VERSION}_x64-setup.nsis.zip"),
        format!("Cross-Platform.Explorer_(Sidecar)_{VERSION}_amd64.AppImage"),
        // CPE-1923: NOT a `.dmg`. Tauri's updater cannot apply a .dmg, and this repo's real
        // published manifest serves `.app.tar.gz` for every darwin key (the .dmg ships as a
        // release asset but no platform entry ever points at it). Also deliberately
        // versionless, exactly as the real macOS updater artifact is named.
        "Cross-Platform.Explorer_(Sidecar)_aarch64.app.tar.gz".to_string(),
    ];
    let platform_keys = ["windows-x86_64", "linux-x86_64", "darwin-aarch64"];
    let mut platforms = serde_json::Map::new();
    for (name, plat) in names.iter().zip(platform_keys.iter()) {
        let bytes = format!("bytes for {name}").into_bytes();
        std::fs::write(root.join(name), &bytes).expect("write artifact");
        let sig = sign_bytes(&kp, &bytes);
        let url = format!("https://example.com/releases/download/v{VERSION}-sidecar/{name}");
        platforms.insert((*plat).to_string(), serde_json::json!({ "signature": sig, "url": url }));
    }
    let manifest = serde_json::json!({ "version": VERSION, "platforms": platforms });
    std::fs::write(root.join("latest.json"), manifest.to_string()).expect("write manifest");

    let conf = serde_json::json!({
        "version": VERSION,
        "productName": "Cross-Platform Explorer", // base conf's real (plain) productName, unchanged
        "plugins": { "updater": { "pubkey": pubkey_config } }
    });
    std::fs::write(root.join("tauri.conf.json"), conf.to_string()).expect("write conf");

    let out = run_with_expect_channel(root, "sidecar");
    assert!(
        out.status.success(),
        "a channel-pure sidecar manifest must pass with --expect-channel sidecar even though --conf's \
         own productName is plain; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("verified 3 of 3 platform signature(s)"));
}
