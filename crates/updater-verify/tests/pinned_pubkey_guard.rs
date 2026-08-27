//! CPE-1873 — fails loudly the moment the **base** `src-tauri/tauri.conf.json`'s
//! `plugins.updater.pubkey` / `.endpoints` change without a matching, same-commit update to the
//! pinned copies in `src/pinned_pubkey.rs`.
//!
//! Precisely where this runs, and — as important — where it does NOT (CPE-1873 round-2 review
//! corrected an earlier overclaim here; this is the accurate version):
//!
//! - Runs via a plain `cargo test -p cpe-updater-verify`, which `ci.yml`'s "updater-verify — clippy +
//!   test" step executes on every push and every PR that reaches `main`.
//! - Does **NOT** run on the tag-push path: `ci.yml` has no `tags:` trigger, so this test never
//!   executes for a tag pointed at a commit that didn't go through `main` (or one CI never evaluated).
//!   `release.yml`'s tag-triggered `verify-published-manifest` job covers that path separately, by
//!   checking the same two constants from inside the `verify-release-artifacts` binary it already
//!   runs — see that binary's `main()` and `src/pinned_pubkey.rs`'s module doc for the full map of
//!   which check runs where.
//! - Does **NOT** look at any `--config` overlay (`tauri.sidecar*.conf.json` etc.) — only the base
//!   file. `src/lib/sidecarBundleResources.test.ts` covers the merged-overlay path, because an
//!   overlay can override these same keys the same way it can override `bundle.resources`.
//! - Does **NOT** look at the per-platform config files Tauri merges *automatically*, with no
//!   `--config` flag at all. That is `tests/platform_config_guard.rs` (CPE-1903), which moved out of
//!   this file when its three-hardcoded-filenames form was replaced by a directory scan — and which
//!   also runs inside `verify-release-artifacts`, so unlike the two guards below it reaches the tag
//!   path.
//!
//! See `src/pinned_pubkey.rs` for the full picture: what all of these checks together prove, what
//! they deliberately do NOT protect against, and the intended key-rotation procedure.

use cpe_updater_verify::{EXPECTED_TAURI_UPDATER_ENDPOINTS, EXPECTED_TAURI_UPDATER_PUBKEY};

/// Locate `src-tauri/tauri.conf.json` relative to this crate, the same way every other guard/binary in
/// this workspace addresses the repo's config: `CARGO_MANIFEST_DIR` is `<repo>/crates/updater-verify`,
/// so the real config is two levels up.
fn repo_tauri_conf_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("src-tauri")
        .join("tauri.conf.json")
}

#[test]
fn live_pubkey_matches_the_pinned_copy() {
    let conf_path = repo_tauri_conf_path();
    let conf_text = std::fs::read_to_string(&conf_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", conf_path.display()));
    let conf_json: serde_json::Value =
        serde_json::from_str(&conf_text).unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", conf_path.display()));

    let live_pubkey = conf_json
        .pointer("/plugins/updater/pubkey")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("{} has no plugins.updater.pubkey", conf_path.display()));

    assert_eq!(
        live_pubkey,
        EXPECTED_TAURI_UPDATER_PUBKEY,
        "\n\n\
         SECURITY (CPE-1873): the updater's root-of-trust public key changed.\n\
         `src-tauri/tauri.conf.json` -> plugins.updater.pubkey no longer matches the pinned copy in\n\
         crates/updater-verify/src/pinned_pubkey.rs::EXPECTED_TAURI_UPDATER_PUBKEY.\n\
         \n\
         If you just performed a DELIBERATE, authorized key rotation: update\n\
         EXPECTED_TAURI_UPDATER_PUBKEY to the same new value in this same commit/PR, and record why in\n\
         the ticket that authorized it. See the rotation procedure documented in\n\
         crates/updater-verify/src/pinned_pubkey.rs and README.md's \"Auto-updates\" section.\n\
         \n\
         If you did NOT intend to change the updater signing key: STOP. Do not edit this constant to\n\
         make the test pass -- find out why tauri.conf.json's pubkey changed before doing anything else.\n\
         This guard exists precisely because a commit that rotates the pubkey and re-signs with the\n\
         matching private key otherwise passes every other check in this repo (CPE-1873).\n"
    );
}


#[test]
fn live_endpoints_match_the_pinned_copy() {
    let conf_path = repo_tauri_conf_path();
    let conf_text = std::fs::read_to_string(&conf_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", conf_path.display()));
    let conf_json: serde_json::Value =
        serde_json::from_str(&conf_text).unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", conf_path.display()));

    let live_endpoints: Vec<String> = conf_json
        .pointer("/plugins/updater/endpoints")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("{} has no plugins.updater.endpoints array", conf_path.display()))
        .iter()
        .map(|v| v.as_str().unwrap_or_default().to_string())
        .collect();
    let expected: Vec<String> = EXPECTED_TAURI_UPDATER_ENDPOINTS.iter().map(|s| s.to_string()).collect();

    assert_eq!(
        live_endpoints,
        expected,
        "

         SECURITY (CPE-1873): the updater's manifest endpoint(s) changed.
         `src-tauri/tauri.conf.json` -> plugins.updater.endpoints no longer matches the pinned copy in
         crates/updater-verify/src/pinned_pubkey.rs::EXPECTED_TAURI_UPDATER_ENDPOINTS.
         
         Repointing this can silently downgrade users to an older, genuinely-signed but vulnerable
         build forever -- even with the pubkey pin fully intact -- so it is guarded the same way.
         
         If you just performed a DELIBERATE, authorized endpoint change: update
         EXPECTED_TAURI_UPDATER_ENDPOINTS to the same new value in this same commit/PR, and record why
         in the ticket that authorized it. See crates/updater-verify/src/pinned_pubkey.rs and
         README.md's \"Auto-updates\" section.
         
         If you did NOT intend to change the updater endpoint(s): STOP. Do not edit this constant to
         make the test pass -- find out why tauri.conf.json's endpoints changed before doing anything
         else (CPE-1873).
"
    );
}
