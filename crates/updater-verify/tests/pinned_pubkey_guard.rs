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
//!
//! See `src/pinned_pubkey.rs` for the full picture: what all of these checks together prove, what
//! they deliberately do NOT protect against, and the intended key-rotation procedure.

use cpe_updater_verify::{EXPECTED_TAURI_UPDATER_ENDPOINTS, EXPECTED_TAURI_UPDATER_PUBKEY};

/// Locate `src-tauri/tauri.conf.json` relative to this crate, the same way every other guard/binary in
/// this workspace addresses the repo's config: `CARGO_MANIFEST_DIR` is `<repo>/crates/updater-verify`,
/// so the real config is two levels up.
fn repo_tauri_conf_path() -> std::path::PathBuf {
    repo_src_tauri_dir().join("tauri.conf.json")
}

/// `<repo>/src-tauri` — shared by every guard in this file that needs to look at more than one file
/// in that directory (CPE-1873 finding 6 below needs the directory itself, not just the base config).
fn repo_src_tauri_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("src-tauri")
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

/// CPE-1873 finding 6 (round 3 — independent Security Auditor, DEMONSTRATED): Tauri merges a
/// per-platform config file AUTOMATICALLY, with no `--config` flag involved at all --
/// `tauri-utils::config::parse::read_from` reads `tauri.conf.json` and then looks for
/// `tauri.macos.conf.json` / `tauri.linux.conf.json` / `tauri.windows.conf.json` next to it and merges
/// each via RFC 7396, unconditionally, on every build. None of the three exists in this repo today, and
/// none was in `CONFIG_CHAIN` (the guard above only knows about overlays release-sidecar.yml explicitly
/// passes via `--config`). Proven: a `src-tauri/tauri.windows.conf.json` containing only a
/// `plugins.updater` override left every other guard in this crate AND
/// `sidecarBundleResources.test.ts` green, while shipping an attacker's pubkey/endpoints on every
/// Windows build -- plain channel and sidecar both, since this file has nothing to do with
/// `--config` and is picked up by a plain `tauri.conf.json` read too.
///
/// This does not try to merge/validate those files' content (that's what the guards above do for the
/// base config) -- it just refuses their EXISTENCE with a `plugins.updater` key, since none is
/// supposed to exist at all right now. If one is ever legitimately introduced, this test's failure
/// message says exactly what to do.
#[test]
fn no_automatic_per_platform_config_overrides_the_updater_pin() {
    let src_tauri = repo_src_tauri_dir();
    for platform_file in ["tauri.windows.conf.json", "tauri.macos.conf.json", "tauri.linux.conf.json"] {
        let path = src_tauri.join(platform_file);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue; // doesn't exist -- the expected, safe state today.
        };
        let json: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{} exists but is not valid JSON: {e}", path.display()));
        assert!(
            json.pointer("/plugins/updater").is_none(),
            "\n\n\
             SECURITY (CPE-1873 finding 6): {} exists and sets plugins.updater.\n\
             Tauri merges this file into the build AUTOMATICALLY (no --config flag needed -- see\n\
             tauri-utils::config::parse::read_from), so it can silently override the pinned\n\
             pubkey/endpoints on every build for that OS, exactly like a --config overlay can, without\n\
             ever appearing in CONFIG_CHAIN or this crate's own --search path.\n\
             \n\
             If this is a deliberate, authorized change: it must not set plugins.updater at all -- put\n\
             any real key/endpoint change through tauri.conf.json (or a --config overlay already in\n\
             CONFIG_CHAIN) so the existing pins actually see it, and record why in the ticket that\n\
             authorized it.\n\
             \n\
             If you did not intend to add this: STOP, this file's plugins.updater block is not\n\
             trustworthy (CPE-1873).\n",
            path.display()
        );
    }
}
