//! CPE-1917 — an **executable** pin on the one fact that broke the plain `Release` workflow for 27
//! days: *where `latest.json` is, and where the verifier is told to look for it, must be the same
//! place*.
//!
//! ## The failure this exists to make impossible again
//!
//! Every run of `.github/workflows/release.yml` from 2026-08-04 to 2026-08-23 failed on all three
//! matrix legs with, byte for byte:
//!
//! ```text
//! verify-release-artifacts: no latest.json found under ../../src-tauri/target (searched: 1)
//! ```
//!
//! and the dependent `catalog` job was `skipped` on every one of them. The cause (CPE-1872) was not
//! a missing manifest — `tauri-action` was producing a complete, correct, fully-signed one all
//! along; it writes it to `resolve(process.cwd(), 'latest.json')`, i.e. the job's repo root, and
//! uploads it straight to the release. It has never been written under `src-tauri/target` on any
//! platform, so the old `--search ../../src-tauri/target` was structurally guaranteed to find
//! nothing. Re-confirmed for this ticket against the real artifacts: `v0.57.69`'s draft carries a
//! 7,206-byte `latest.json` naming 11 platforms, and running today's gate over the real published
//! manifest plus all six referenced assets exits 0 with "verified 11 of 11".
//!
//! ## Why a *new* pin, when CPE-1872 already added tests
//!
//! CPE-1872's `release_guard.rs` tests hard-code the invocation they claim mirrors `release.yml`.
//! That claim was true for about one commit: round 2 of the same ticket moved the check out of the
//! matrix into the post-matrix `verify-published-manifest` job, with entirely different arguments
//! (`--manifest release-assets/latest.json --search release-assets`, over assets downloaded from the
//! draft release), and the tests' hard-coded `--manifest latest.json --search src-tauri/target` was
//! never updated. So the guard for "the workflow points at the right place" stopped being about the
//! workflow at all — it is green today no matter what `release.yml` says. That is the exact defect
//! this repo keeps re-finding (CPE-1929: a guard that could be deleted outright with thousands of
//! tests still green), applied to the very ticket that was meant to close it.
//!
//! ## How this file cannot rot the same way
//!
//! Nothing here is hard-coded. The two halves of the invariant are read out of `release.yml` from
//! **two different places**, and then actually executed against each other:
//!
//! 1. `download_dir()` reads where the *download* step puts `latest.json` (`gh release download …
//!    --pattern 'latest.json' --dir <DIR>`).
//! 2. `verify_argv()` reads the argv the *verify* step passes to this crate's binary.
//! 3. The test scaffolds a repo-shaped temp tree with the manifest + artifact under (1), runs the
//!    real binary with (2), and requires exit 0.
//!
//! Because the scaffold comes from the download step and the argv from the verify step, the test is
//! not circular: move `latest.json` in *either* half without moving it in the other and the binary
//! reports "no latest.json found" and this test goes red — locally, on every PR, in seconds, instead
//! of on the next version tag someone happens to push a month later. Reverting the verify step to the
//! pre-CPE-1872 `--search src-tauri/target` fails it for the same reason, which is the specific
//! regression the ticket asks to be pinned.
//!
//! The structural half of the same invariant (that the verify step lives in `verify-published-
//! manifest` rather than the matrix, that its job gate is `if: ${{ !cancelled() }}`, and that the
//! failure-watchdog still names both release workflows) is asserted in
//! `src/lib/releaseVerifyWiringGuard.test.ts`, which parses the YAML properly. This file deliberately
//! stays text-based: `cpe-updater-verify` has no YAML dependency and this ticket adds none.

use std::path::Path;
use std::process::Command;

use base64::Engine as _;
use minisign::KeyPair;

const B64: base64::engine::general_purpose::GeneralPurpose = base64::engine::general_purpose::STANDARD;
const BIN: &str = env!("CARGO_BIN_EXE_verify-release-artifacts");

/// Fixture values substituted for the workflow's `${REPO}` / `${TAG}` shell expansions.
const FIXTURE_REPO: &str = "StewartScottRogers/cross-platform-explorer";
const FIXTURE_TAG: &str = "v1.2.3";
const FIXTURE_VERSION: &str = "1.2.3";
/// Plain-channel installer name — must not read as sidecar, or CPE-1894's channel check rejects it.
const ARTIFACT_NAME: &str = "Cross-Platform.Explorer_1.2.3_x64-setup.exe";
const PRODUCT_NAME: &str = "Cross-Platform Explorer";

fn workflow_text() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(".github")
        .join("workflows")
        .join("release.yml");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Collapse a backslash-continued shell command starting at `lines[start]` into one logical line.
fn logical_line(lines: &[&str], start: usize) -> String {
    let mut out = String::new();
    let mut i = start;
    loop {
        let raw = lines[i].trim_end();
        let body = raw.strip_suffix('\\').unwrap_or(raw);
        out.push_str(body.trim());
        out.push(' ');
        if !raw.ends_with('\\') || i + 1 >= lines.len() {
            break;
        }
        i += 1;
    }
    out
}

/// Split a shell-ish command into tokens, dropping the quoting around each one. No token in the
/// invocations this reads contains whitespace, so a whitespace split plus quote-stripping is exact;
/// a future token that *did* need embedded whitespace would show up as a bogus extra token and fail
/// the assertions below rather than passing silently.
fn tokens(line: &str) -> Vec<String> {
    line.split_whitespace()
        .map(|t| t.trim_matches(|c| c == '"' || c == '\'').to_string())
        .collect()
}

/// Where the `verify-published-manifest` download step deposits the published `latest.json`.
fn download_dir(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let idx = lines
        .iter()
        .position(|l| l.contains("gh release download") && l.contains("latest.json"))
        .expect(
            "release.yml no longer has a `gh release download … latest.json` step in \
             verify-published-manifest -- if the published manifest is fetched some other way now, \
             update this guard to read the new shape rather than deleting it (CPE-1917)",
        );
    let toks = tokens(&logical_line(&lines, idx));
    let dir = toks
        .iter()
        .position(|t| t == "--dir")
        .and_then(|i| toks.get(i + 1))
        .expect("the latest.json download step passes no --dir")
        .clone();
    assert!(
        !dir.starts_with('-') && !dir.contains("$"),
        "the download step's --dir must be a literal path this guard can scaffold, got {dir:?}"
    );
    dir
}

/// The argv `release.yml` hands to `verify-release-artifacts`, `${REPO}`/`${TAG}` resolved.
fn verify_argv(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let hits: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains("--bin verify-release-artifacts"))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "release.yml must invoke verify-release-artifacts exactly once. More than one means the \
         per-matrix-leg check CPE-1872 deleted has crept back (it verified a fragment of a manifest \
         that is the union of all three legs, and reported success on it); none means the release \
         gate is gone entirely."
    );
    let toks = tokens(&logical_line(&lines, hits[0]));
    let dashdash = toks
        .iter()
        .position(|t| t == "--")
        .expect("the cargo run invocation has no `--` separating cargo's args from the binary's");
    toks[dashdash + 1..]
        .iter()
        .map(|t| {
            t.replace("${REPO}", FIXTURE_REPO)
                .replace("${TAG}", FIXTURE_TAG)
        })
        .collect()
}

fn flag_value<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
    argv.iter()
        .position(|a| a == flag)
        .and_then(|i| argv.get(i + 1))
        .map(String::as_str)
}

/// Build a temp tree shaped like the `verify-published-manifest` job's workspace: the manifest and
/// its artifact under whatever directory the *download step* names, and the conf where `--conf`
/// says. Signature is computed over `signed_bytes`; `artifact_on_disk` is what actually lands, so
/// passing different values simulates tampering.
fn scaffold(manifest_dir: &str, conf_rel: &str, signed_bytes: &[u8], artifact_on_disk: &[u8]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    let assets = root.join(manifest_dir);
    std::fs::create_dir_all(&assets).expect("mkdir manifest dir");

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

    std::fs::write(assets.join(ARTIFACT_NAME), artifact_on_disk).expect("write artifact");

    let manifest = serde_json::json!({
        "version": FIXTURE_VERSION,
        "platforms": {
            "windows-x86_64": {
                "signature": signature_field,
                "url": format!(
                    "https://github.com/{FIXTURE_REPO}/releases/download/{FIXTURE_TAG}/{ARTIFACT_NAME}"
                )
            }
        }
    });
    std::fs::write(assets.join("latest.json"), manifest.to_string()).expect("write manifest");

    let conf_path = root.join(conf_rel);
    if let Some(parent) = conf_path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir conf dir");
    }
    let conf = serde_json::json!({
        "version": FIXTURE_VERSION,
        "productName": PRODUCT_NAME,
        "plugins": { "updater": { "pubkey": pubkey_config } }
    });
    std::fs::write(&conf_path, conf.to_string()).expect("write conf");

    dir
}

/// Run the binary from `root` with the workflow's own argv. `--skip-pin-check` is appended because
/// these fixtures sign with a throwaway keypair unrelated to the repo's real pinned pubkey — the pin
/// itself is covered by `pinned_pubkey_guard.rs` and, in production, by the real invocation. The
/// assertions below prove the *workflow* never passes it.
fn run_with_workflow_argv(root: &Path, argv: &[String]) -> std::process::Output {
    let mut cmd = Command::new(BIN);
    cmd.current_dir(root);
    cmd.args(argv);
    cmd.arg("--skip-pin-check");
    cmd.output().expect("run verify-release-artifacts")
}

// ---------------------------------------------------------------------------------------------
// Static assertions about the argv itself
// ---------------------------------------------------------------------------------------------

#[test]
fn the_verify_step_reads_the_manifest_from_the_directory_the_download_step_writes_it_to() {
    let text = workflow_text();
    let dir = download_dir(&text);
    let argv = verify_argv(&text);

    let manifest = flag_value(&argv, "--manifest").expect(
        "the verify step passes no --manifest. Without it the binary falls back to \"newest \
         latest.json found under --search\", which is precisely the discovery-by-luck that failed \
         every run for 27 days (CPE-1917).",
    );
    assert_eq!(
        manifest,
        format!("{dir}/latest.json"),
        "--manifest must name the file the download step actually fetched. The download step writes \
         latest.json into `{dir}`; the verify step is looking somewhere else, which is the CPE-1917 \
         failure verbatim."
    );

    let search = flag_value(&argv, "--search").expect("the verify step passes no --search");
    assert_eq!(
        search, dir,
        "--search must be the directory the referenced assets were downloaded into. `src-tauri/target` \
         (the pre-CPE-1872 value) never contains them: tauri-action uploads its bundles to the release \
         and this job re-downloads them, so pointing the search at a build directory finds nothing."
    );
}

#[test]
fn the_verify_step_keeps_its_url_binding_and_never_disarms_the_pin() {
    let text = workflow_text();
    let argv = verify_argv(&text);

    let prefix = flag_value(&argv, "--expect-url-prefix").expect(
        "the verify step dropped --expect-url-prefix. Without it the crypto check only proves the \
         artifact BYTES are genuine, never that the url a real updater fetches points at this repo's \
         own release -- CPE-1872 round 3 finding B, reproduced at exit 0 against a foreign host and \
         against the wrong tag.",
    );
    assert_eq!(
        prefix,
        format!("https://github.com/{FIXTURE_REPO}/releases/download/{FIXTURE_TAG}/"),
        "--expect-url-prefix must resolve to this repo's real releases/download/<tag>/ prefix"
    );

    assert!(
        !argv.iter().any(|a| a == "--skip-pin-check"),
        "release.yml must never pass --skip-pin-check: that flag exists only for this crate's own \
         throwaway-keypair fixtures, and passing it on the tag path would disarm the CPE-1873 \
         pubkey/endpoints pin on the one invocation that guards a real release."
    );

    assert_eq!(
        flag_value(&argv, "--conf"),
        Some("src-tauri/tauri.conf.json"),
        "--conf must be the plain channel's real config -- it is what supplies the pinned pubkey, the \
         version the manifest is checked against, and (CPE-1894) the channel the assets must belong to"
    );
}

// ---------------------------------------------------------------------------------------------
// The executable half: run the real binary with the real argv
// ---------------------------------------------------------------------------------------------

/// GREEN. The workflow's own argv, run against a tree laid out the way the workflow's own download
/// step lays one out, verifies clean. This is the assertion that goes red the moment either half
/// moves without the other.
#[test]
fn the_workflows_own_argv_verifies_a_tree_laid_out_by_the_workflows_own_download_step() {
    let text = workflow_text();
    let dir = download_dir(&text);
    let argv = verify_argv(&text);
    let conf = flag_value(&argv, "--conf").expect("--conf").to_string();

    let bytes = b"the real installer bytes";
    let tree = scaffold(&dir, &conf, bytes, bytes);
    let out = run_with_workflow_argv(tree.path(), &argv);

    assert!(
        out.status.success(),
        "release.yml's own verify invocation failed against a workspace laid out by release.yml's own \
         download step -- the two halves of the manifest's location disagree (CPE-1917).\n\
         --- argv ---\n{argv:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

/// RED, reproducing the original bug exactly: put the manifest where the *pre-fix* workflow looked
/// (`src-tauri/target`) instead of where the download step puts it, and the current argv must fail
/// with the same message every real run produced for 27 days. Proves the GREEN test above is
/// discriminating rather than passing on anything at all.
#[test]
fn a_manifest_left_under_src_tauri_target_is_not_found_by_the_current_argv() {
    let text = workflow_text();
    let argv = verify_argv(&text);
    let conf = flag_value(&argv, "--conf").expect("--conf").to_string();

    let bytes = b"the real installer bytes";
    let tree = scaffold("src-tauri/target/release/bundle/nsis", &conf, bytes, bytes);
    let out = run_with_workflow_argv(tree.path(), &argv);

    assert!(
        !out.status.success(),
        "a manifest under src-tauri/target -- the location the broken workflow searched, and the one \
         tauri-action has never written to -- must NOT satisfy the current invocation"
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("latest.json"),
        "the failure must still name the missing manifest, the way the real runs did.\n{combined}"
    );
}

/// RED. The signature check is still live under the workflow's real argv -- a tampered artifact in
/// the download directory fails, so the GREEN test above is not passing merely because the files
/// exist.
#[test]
fn a_tampered_artifact_still_fails_under_the_workflows_own_argv() {
    let text = workflow_text();
    let dir = download_dir(&text);
    let argv = verify_argv(&text);
    let conf = flag_value(&argv, "--conf").expect("--conf").to_string();

    let tree = scaffold(&dir, &conf, b"the real installer bytes", b"tampered bytes");
    let out = run_with_workflow_argv(tree.path(), &argv);

    assert!(
        !out.status.success(),
        "a tampered artifact must fail the release gate under the workflow's own invocation"
    );
}
