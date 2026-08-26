//! CPE-1873 — fails loudly the moment `src-tauri/tauri.conf.json`'s `plugins.updater.pubkey` changes
//! without a matching, same-commit update to the pinned copy in `src/pinned_pubkey.rs`.
//!
//! This is deliberately a plain `cargo test` in this crate, not a shell grep in a workflow file: it
//! runs on every push/PR to `main` via `ci.yml`'s "updater-verify — clippy + test" step (see that
//! step's own comment), so a commit that touches ONLY `tauri.conf.json`'s pubkey fails CI before it
//! ever reaches a release tag — not just when `release.yml`'s tag-triggered jobs happen to run.
//!
//! See `src/pinned_pubkey.rs` for what this proves, what it deliberately does NOT protect against, and
//! the intended key-rotation procedure.

use cpe_updater_verify::EXPECTED_TAURI_UPDATER_PUBKEY;

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
