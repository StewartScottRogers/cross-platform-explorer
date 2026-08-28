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
//! 1. `download_calls()` reads where the *download* step stages what it fetches. There are **two**
//!    `gh release download` calls — one for `latest.json`, one looping over the assets that manifest
//!    references — and they must agree on `--dir`, because `--search` names a single directory and a
//!    manifest separated from its artifacts fails every platform as unavailable. Reading only the
//!    first was this guard's own round-1 hole (CPE-1917 round 2, Reviewer): changing only the second
//!    left everything green while the real job would break on its next tag.
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

/// The plain release workflow, and (CPE-1933) the sidecar one. Both are read by name so every
/// assertion below is derived from the file it names rather than asserted about it in prose.
///
/// **CPE-1969 asked whether this two-file scope is deliberate or another remembered list, and the
/// answer is deliberate — with one half that had to be moved from prose into code.**
///
/// Deliberate, because this file is not a "scan all the X in the repo" guard at all. It asks a
/// question about ONE named invocation: it reads `release.yml`'s and `release-sidecar.yml`'s own
/// argv out of the workflow text and then *executes the real binary with it* against a scaffolded
/// tree. There is nothing to enumerate — "every workflow" is not the subject, "the release gate" is,
/// and a release gate exists in exactly the workflow that publishes a channel. Deriving the file
/// list here would only find files with no argv to read, and each would have to be skipped, which is
/// the same two names written backwards.
///
/// The half that WAS a remembered list is the implicit claim that no OTHER workflow — or extracted
/// `.sh` script — invokes `verify-release-artifacts` where none of this scrutiny reaches it. That
/// claim is now derived rather than assumed, in `src/lib/channelPurityCoverage.test.ts` ("no
/// workflow or extracted script outside the mapped set invokes verify-release-artifacts"), which
/// walks `workflowShellSources.allShellUnits()` — every workflow step plus all three scripts. It
/// lives on the TypeScript side because that is where the workflow enumeration and the YAML parser
/// already are; duplicating it here would be a second implementation of exactly the enumeration
/// CPE-1969 exists to stop having two of.
///
/// The same verdict applies to `src/artifact_binding.rs`'s workflow derivation, which reads
/// `release-sidecar.yml`'s verify step to recover the real `(channel, --conf productName)` pair: it
/// derives one specific fact from the one file that states it, not a sweep over a file class.
const RELEASE_YML: &str = "release.yml";
const RELEASE_SIDECAR_YML: &str = "release-sidecar.yml";

fn workflow_text(file: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(".github")
        .join("workflows")
        .join(file);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The workflow's **logical shell lines**: `#` comments stripped (quote-, escape- and word-boundary
/// aware), backslash continuations joined, heredoc bodies skipped.
///
/// CPE-1933: every scanner below anchors on a substring — `gh release download`,
/// `--bin verify-release-artifacts`. A *comment* containing that substring is otherwise parsed as if
/// it were the real thing. Not hypothetical: `release-sidecar.yml` has two prose comments (`:665`,
/// `:734`) that mention `gh release download` while discussing it, and a `<<'EOF'` heredoc at `:71`.
///
/// This delegates to [`cpe_updater_verify::workflow_scan`] rather than filtering here. CPE-1933's
/// first draft did filter here, and blanked only comment-*only* lines — so a **trailing** comment
/// walked straight through it:
///
/// ```text
/// --expect-url-prefix "https://…/${TAG}/"  # was: --expect-channel sidecar
/// ```
///
/// read the flag out of the comment and passed, which is PR #1056's hole reproduced in the very
/// assertion written to close it. The repo already had the right stripper
/// (`src/lib/shellScriptLines.ts`, extracted at CPE-1849 and hardened through CPE-1908 rounds 2/3
/// precisely so a second hand-rolled one could not disagree with it); `workflow_scan` is its Rust
/// port, pinned to it by a shared case file.
fn logical_lines(text: &str) -> Vec<String> {
    cpe_updater_verify::workflow_scan::logical_lines(text)
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

/// Every `gh release download` call in the workflow, as `(fetches_the_manifest, --dir value)`.
///
/// CPE-1917 round 2 (Reviewer, MEDIUM): there are **two** of these, not one. One fetches
/// `latest.json`; the other loops over the names that manifest references and fetches each installer.
/// They must land in the SAME directory, because `--search` names a single directory and the verifier
/// needs the manifest and the artifacts it points at together. The first version of this guard read
/// only the manifest call, so changing only the *second* — manifest into `release-assets/`, its
/// installers into `downloaded/` — left every assertion green while the real job would fail on its
/// next tag with no artifact bytes to verify: the identical class of outage this file exists to
/// prevent, and a hole the Reviewer walked straight through.
fn download_calls(wf: &str, text: &str) -> Vec<(bool, String)> {
    let lines = logical_lines(text);
    let calls: Vec<(bool, String)> = lines
        .iter()
        .filter(|l| l.contains("gh release download"))
        .map(|line| {
            let toks = tokens(line);
            let dir = toks
                .iter()
                .position(|t| t == "--dir")
                .and_then(|j| toks.get(j + 1))
                .unwrap_or_else(|| panic!("a `gh release download` call passes no --dir: {line}"))
                .clone();
            assert!(
                !dir.starts_with('-') && !dir.contains('$'),
                "every download --dir must be a literal path this guard can scaffold, got {dir:?}"
            );
            (line.contains("latest.json"), dir)
        })
        .collect();
    assert!(
        calls.iter().any(|(is_manifest, _)| *is_manifest),
        "{wf} no longer has a `gh release download … latest.json` call in \
         verify-published-manifest -- if the published manifest is fetched some other way now, update \
         this guard to read the new shape rather than deleting it (CPE-1917)"
    );
    assert!(
        calls.iter().any(|(is_manifest, _)| !*is_manifest),
        "{wf} no longer downloads the assets the manifest REFERENCES, only the manifest \
         itself. `--search` would then hold a manifest with no artifact bytes behind it, and every \
         platform would fail as unavailable (CPE-1917 round 2)."
    );
    calls
}

/// The one directory the whole `verify-published-manifest` job stages into. Panics if the download
/// calls disagree — that disagreement *is* the bug, so it must never be quietly resolved to one of
/// them.
fn download_dir(wf: &str, text: &str) -> String {
    let calls = download_calls(wf, text);
    let first = calls[0].1.clone();
    assert!(
        calls.iter().all(|(_, dir)| *dir == first),
        "{wf}'s `gh release download` calls stage into different directories ({:?}). The \
         manifest and the artifacts it names must land together: `--search` is a single directory, \
         and a manifest whose artifacts are somewhere else fails every platform as unavailable — \
         which is this ticket's outage wearing a different hat.",
        calls.iter().map(|(_, d)| d.as_str()).collect::<Vec<_>>(),
    );
    first
}

/// The argv `wf` hands to `verify-release-artifacts`, `${REPO}`/`${TAG}` resolved.
fn verify_argv(wf: &str, text: &str) -> Vec<String> {
    let lines = logical_lines(text);
    let hits: Vec<&String> = lines
        .iter()
        .filter(|l| l.contains("--bin verify-release-artifacts"))
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "{wf} must invoke verify-release-artifacts exactly once. More than one means the \
         per-matrix-leg check CPE-1872 deleted has crept back (it verified a fragment of a manifest \
         that is the union of all three legs, and reported success on it); none means the release \
         gate is gone entirely."
    );
    let toks = tokens(hits[0]);
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

/// The single value of `flag` in `argv`.
///
/// CPE-1917 round 2 (Reviewer, LOW-MED): this used to return the FIRST occurrence, so appending a
/// second `--search src-tauri/target` to the verify step satisfied every assertion below while
/// re-opening CPE-1872's own round-2 finding — a stale, same-basename artifact in a second, dirty
/// search dir shadowing the freshly-downloaded one. A repeated flag is never intentional in this
/// invocation, so it is a hard failure rather than a silently-ignored extra.
fn flag_value<'a>(wf: &str, argv: &'a [String], flag: &str) -> Option<&'a str> {
    let hits: Vec<usize> = argv
        .iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == flag)
        .map(|(i, _)| i)
        .collect();
    assert!(
        hits.len() <= 1,
        "{wf} passes {flag} {} times. Every flag in this invocation means exactly one thing; \
         a second copy either silently overrides the first or widens what the verifier will trust.",
        hits.len()
    );
    hits.first().and_then(|i| argv.get(i + 1)).map(String::as_str)
}

/// Build a temp tree shaped like the `verify-published-manifest` job's workspace: the manifest and
/// its artifact under whatever directory the *download step* names, and the conf where `--conf`
/// says. Signature is computed over `signed_bytes`; `artifact_on_disk` is what actually lands, so
/// passing different values simulates tampering.
/// `asset_name` is the installer basename the manifest points at. It is a parameter (CPE-1933)
/// because the *channel* a manifest belongs to is read off this name, so the sidecar half below
/// needs a sidecar-named asset while `--conf`'s `productName` stays plain — which is precisely the
/// pairing `release-sidecar.yml` uses and the pairing three prose comments used to merely claim.
fn scaffold(
    manifest_dir: &str,
    conf_rel: &str,
    asset_name: &str,
    signed_bytes: &[u8],
    artifact_on_disk: &[u8],
) -> tempfile::TempDir {
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
        // CPE-1923/SEC-1: the anti-rollback decision reads the trusted comment's `file:` field, so
        // this fixture carries the real shape a Tauri signature has.
        Some(&format!("timestamp:1787496720	file:{asset_name}")),
        Some("untrusted"),
    )
    .expect("sign");
    let signature_field = B64.encode(sig.into_string().as_bytes());

    std::fs::write(assets.join(asset_name), artifact_on_disk).expect("write artifact");

    let manifest = serde_json::json!({
        "version": FIXTURE_VERSION,
        "platforms": {
            "windows-x86_64": {
                "signature": signature_field,
                "url": format!(
                    "https://github.com/{FIXTURE_REPO}/releases/download/{FIXTURE_TAG}/{asset_name}"
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
    let text = workflow_text(RELEASE_YML);
    let dir = download_dir(RELEASE_YML, &text);
    let argv = verify_argv(RELEASE_YML, &text);

    let manifest = flag_value(RELEASE_YML, &argv, "--manifest").expect(
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

    let search = flag_value(RELEASE_YML, &argv, "--search").expect("the verify step passes no --search");
    assert_eq!(
        search, dir,
        "--search must be the directory the referenced assets were downloaded into. `src-tauri/target` \
         (the pre-CPE-1872 value) never contains them: tauri-action uploads its bundles to the release \
         and this job re-downloads them, so pointing the search at a build directory finds nothing."
    );
}

/// CPE-1917 round 2 (Reviewer, MEDIUM): the manifest and the artifacts it names are fetched by two
/// separate `gh release download` calls, and only landing them in one directory makes `--search`
/// meaningful. Asserted on its own, not just as a side effect of `download_dir`'s internals, so the
/// failure names the real problem.
#[test]
fn both_download_calls_stage_into_the_same_directory() {
    let text = workflow_text(RELEASE_YML);
    let calls = download_calls(RELEASE_YML, &text);
    assert!(
        calls.len() >= 2,
        "expected at least two `gh release download` calls (the manifest, and the assets it \
         references); found {}",
        calls.len()
    );
    let dirs: Vec<&str> = calls.iter().map(|(_, d)| d.as_str()).collect();
    assert!(
        dirs.windows(2).all(|w| w[0] == w[1]),
        "the manifest download and the referenced-asset download disagree on --dir: {dirs:?}"
    );
    // And the directory they agree on is the one the verifier is actually pointed at.
    assert_eq!(flag_value(RELEASE_YML, &verify_argv(RELEASE_YML, &text), "--search"), Some(dirs[0]));
}

/// CPE-1917 round 2 (Reviewer, LOW-MED). `flag_value` only refuses a repeat of a flag some test
/// happens to ask for; this refuses a repeat of ANY of them, including one nothing else reads.
#[test]
fn no_flag_is_passed_more_than_once() {
    let argv = verify_argv(RELEASE_YML, &workflow_text(RELEASE_YML));
    let mut seen: Vec<&str> = Vec::new();
    for flag in argv.iter().filter(|a| a.starts_with("--")) {
        assert!(
            !seen.contains(&flag.as_str()),
            "release.yml passes {flag} more than once in argv {argv:?}. A second `--search` in \
             particular re-opens CPE-1872's round-2 finding: a stale same-basename artifact in a \
             dirty second directory can shadow the freshly-downloaded one and still verify clean."
        );
        seen.push(flag);
    }
}

#[test]
fn the_verify_step_keeps_its_url_binding_and_never_disarms_the_pin() {
    let text = workflow_text(RELEASE_YML);
    let argv = verify_argv(RELEASE_YML, &text);

    let prefix = flag_value(RELEASE_YML, &argv, "--expect-url-prefix").expect(
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
        flag_value(RELEASE_YML, &argv, "--conf"),
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
    let text = workflow_text(RELEASE_YML);
    let dir = download_dir(RELEASE_YML, &text);
    let argv = verify_argv(RELEASE_YML, &text);
    let conf = flag_value(RELEASE_YML, &argv, "--conf").expect("--conf").to_string();

    let bytes = b"the real installer bytes";
    let tree = scaffold(&dir, &conf, ARTIFACT_NAME, bytes, bytes);
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
    let text = workflow_text(RELEASE_YML);
    let argv = verify_argv(RELEASE_YML, &text);
    let conf = flag_value(RELEASE_YML, &argv, "--conf").expect("--conf").to_string();

    let bytes = b"the real installer bytes";
    let tree = scaffold("src-tauri/target/release/bundle/nsis", &conf, ARTIFACT_NAME, bytes, bytes);
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
    let text = workflow_text(RELEASE_YML);
    let dir = download_dir(RELEASE_YML, &text);
    let argv = verify_argv(RELEASE_YML, &text);
    let conf = flag_value(RELEASE_YML, &argv, "--conf").expect("--conf").to_string();

    let tree = scaffold(&dir, &conf, ARTIFACT_NAME, b"the real installer bytes", b"tampered bytes");
    let out = run_with_workflow_argv(tree.path(), &argv);

    assert!(
        !out.status.success(),
        "a tampered artifact must fail the release gate under the workflow's own invocation"
    );
}

// ---------------------------------------------------------------------------------------------
// CPE-1933: the same treatment for `release-sidecar.yml`
//
// Three comments claimed, in prose, that a hard-coded test argv reproduced the sidecar job's:
//
//   * `tests/release_guard.rs` -- "this reproduces exactly what `release-sidecar.yml`'s job
//     checks for"
//   * `tests/release_guard.rs` -- "`--conf` still the base plain-productName conf (exactly as
//     `release-sidecar.yml` invokes it)"
//   * `tests/hostile_manifests.rs` -- "invoked exactly as `release-sidecar.yml` invokes it"
//
// All three were untested by construction, and two of them were already drifting: the real job
// passes `--manifest release-assets/latest.json` and `--expect-url-prefix`, neither of which
// `release_guard.rs`'s `run_with_expect_channel` helper passes at all. That is CPE-1872's defect
// verbatim -- a green test vouching for a claim nobody checks -- surviving in the very file CPE-1917
// corrected one comment in.
//
// The load-bearing half of what those comments assert is a *pairing*, and it is the one a
// well-meaning "fix" would break: the sidecar job points `--conf` at the BASE
// `src-tauri/tauri.conf.json` (plain `productName`, because that is where the pinned pubkey and the
// version live -- the sidecar overlay is a partial that touches neither) while asking for
// `--expect-channel sidecar`. Swap `--conf` to a sidecar overlay and every hard-coded unit test
// above still passes while the real job's channel check inverts. So it is derived and executed here
// instead of described there.
// ---------------------------------------------------------------------------------------------

/// A genuine sidecar installer basename. Tauri's bundler renders the sidecar `productName`
/// (`Cross-Platform Explorer (Sidecar)`) with its punctuation flattened, so the channel classifier
/// reads `crossplatformexplorersidecar` off the front of this name.
const SIDECAR_ARTIFACT_NAME: &str = "Cross-Platform.Explorer.Sidecar._1.2.3_x64-setup.exe";

#[test]
fn the_sidecar_verify_step_reads_the_manifest_from_the_directory_its_download_step_writes_it_to() {
    let text = workflow_text(RELEASE_SIDECAR_YML);
    let dir = download_dir(RELEASE_SIDECAR_YML, &text);
    let argv = verify_argv(RELEASE_SIDECAR_YML, &text);

    let manifest = flag_value(RELEASE_SIDECAR_YML, &argv, "--manifest").expect(
        "the sidecar verify step passes no --manifest. Without it the binary falls back to \"newest \
         latest.json found under --search\" -- the discovery-by-luck that failed every plain release \
         for 27 days (CPE-1917), which the sidecar channel has no immunity to.",
    );
    assert_eq!(
        manifest,
        format!("{dir}/latest.json"),
        "--manifest must name the file release-sidecar.yml's own download step fetched"
    );
    assert_eq!(
        flag_value(RELEASE_SIDECAR_YML, &argv, "--search"),
        Some(dir.as_str()),
        "--search must be the directory release-sidecar.yml downloads the referenced assets into"
    );
}

/// The pairing itself, read straight out of the workflow. This is the assertion that replaces
/// `release_guard.rs`'s and `hostile_manifests.rs`'s prose.
#[test]
fn the_sidecar_job_checks_the_sidecar_channel_using_the_base_plain_conf() {
    let text = workflow_text(RELEASE_SIDECAR_YML);
    let argv = verify_argv(RELEASE_SIDECAR_YML, &text);

    assert_eq!(
        flag_value(RELEASE_SIDECAR_YML, &argv, "--expect-channel"),
        Some("sidecar"),
        "release-sidecar.yml must pass --expect-channel sidecar. Without it CPE-1894's channel-purity \
         check has nothing to check against on the channel that actually reaches users, and a plain \
         asset could ride out in a sidecar manifest at exit 0 -- the whole point of that job."
    );
    assert_eq!(
        flag_value(RELEASE_SIDECAR_YML, &argv, "--conf"),
        Some("src-tauri/tauri.conf.json"),
        "release-sidecar.yml must point --conf at the BASE conf, not a sidecar overlay. \
         `tauri.sidecar.conf.json` is a partial overlay that carries neither the pinned pubkey nor \
         the version, and its `productName` would make the channel classifier read every honest \
         sidecar asset as plain -- rejecting 100% of real sidecar releases (CPE-1908). Several unit \
         tests scaffold a plain-productName conf specifically because this line says so; if this \
         changes, they are testing a shape that no longer ships."
    );
    assert!(
        !argv.iter().any(|a| a == "--skip-pin-check"),
        "release-sidecar.yml must never pass --skip-pin-check -- that flag exists only for this \
         crate's throwaway-keypair fixtures and would disarm the CPE-1873 pin on a real release."
    );
    flag_value(RELEASE_SIDECAR_YML, &argv, "--expect-url-prefix").expect(
        "the sidecar verify step dropped --expect-url-prefix, so its crypto check no longer proves \
         the url a real updater fetches points at this repo's own release (CPE-1872 round 3).",
    );
}

/// GREEN, executable. `release-sidecar.yml`'s own argv, run against a tree laid out by
/// `release-sidecar.yml`'s own download step, carrying a genuine sidecar-named asset and the plain
/// base conf. Exit 0. Move either half without the other and this goes red.
#[test]
fn the_sidecar_workflows_own_argv_accepts_a_genuine_sidecar_release() {
    let text = workflow_text(RELEASE_SIDECAR_YML);
    let dir = download_dir(RELEASE_SIDECAR_YML, &text);
    let argv = verify_argv(RELEASE_SIDECAR_YML, &text);
    let conf = flag_value(RELEASE_SIDECAR_YML, &argv, "--conf").expect("--conf").to_string();

    let bytes = b"the real sidecar installer bytes";
    let tree = scaffold(&dir, &conf, SIDECAR_ARTIFACT_NAME, bytes, bytes);
    let out = run_with_workflow_argv(tree.path(), &argv);

    assert!(
        out.status.success(),
        "release-sidecar.yml's own verify invocation failed against a workspace laid out by \
         release-sidecar.yml's own download step (CPE-1933).\n\
         --- argv ---\n{argv:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

/// RED, and the one that makes the GREEN above discriminating: the SAME plain base conf and the same
/// workflow argv, but a PLAIN-named asset in the manifest. `--expect-channel sidecar` must reject it.
/// This is what `release_guard.rs`'s hard-coded
/// `a_plain_asset_in_a_manifest_expected_sidecar_is_rejected_by_name` asserts about a shape it
/// invented; here the shape comes from the workflow.
#[test]
fn a_plain_asset_is_rejected_under_the_sidecar_workflows_own_argv() {
    let text = workflow_text(RELEASE_SIDECAR_YML);
    let dir = download_dir(RELEASE_SIDECAR_YML, &text);
    let argv = verify_argv(RELEASE_SIDECAR_YML, &text);
    let conf = flag_value(RELEASE_SIDECAR_YML, &argv, "--conf").expect("--conf").to_string();

    let bytes = b"a plain installer smuggled into the sidecar release";
    // ARTIFACT_NAME is the plain-channel basename -- everything else about this tree is honest.
    let tree = scaffold(&dir, &conf, ARTIFACT_NAME, bytes, bytes);
    let out = run_with_workflow_argv(tree.path(), &argv);

    assert!(
        !out.status.success(),
        "a plain-channel asset in the sidecar release must fail under the sidecar job's own argv. \
         Passing means --expect-channel is absent or inert in the real workflow.\n\
         --- argv ---\n{argv:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("release channel"),
        "the refusal must be the CHANNEL check, not an unrelated failure that happens to be \
         non-zero -- otherwise this test would pass even with --expect-channel removed. stderr={stderr}"
    );
}

/// RED. The signature check is still live under the sidecar job's real argv, so the GREEN test is
/// not passing merely because the files exist.
#[test]
fn a_tampered_artifact_still_fails_under_the_sidecar_workflows_own_argv() {
    let text = workflow_text(RELEASE_SIDECAR_YML);
    let dir = download_dir(RELEASE_SIDECAR_YML, &text);
    let argv = verify_argv(RELEASE_SIDECAR_YML, &text);
    let conf = flag_value(RELEASE_SIDECAR_YML, &argv, "--conf").expect("--conf").to_string();

    let tree = scaffold(&dir, &conf, SIDECAR_ARTIFACT_NAME, b"genuine bytes", b"tampered bytes");
    let out = run_with_workflow_argv(tree.path(), &argv);

    assert!(!out.status.success(), "a tampered sidecar artifact must fail the release gate");
}

/// Both release workflows are read by this file, and each must still be invoking the verifier. A
/// workflow renamed or a gate deleted shows up here as a missing file rather than as a silently
/// shrinking test suite (CPE-1933; the CPE-1929 "guard that cannot go red" family).
#[test]
fn both_release_workflows_still_gate_on_the_verifier() {
    for wf in [RELEASE_YML, RELEASE_SIDECAR_YML] {
        let text = workflow_text(wf);
        let argv = verify_argv(wf, &text);
        assert!(!argv.is_empty(), "{wf} invokes verify-release-artifacts with no arguments at all");
    }
}
