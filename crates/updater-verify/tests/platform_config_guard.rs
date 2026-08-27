//! CPE-1903 — the PR-time signal for `cpe_updater_verify::platform_config_guard`, run against the
//! **real** `src-tauri/` directory.
//!
//! Split out of `pinned_pubkey_guard.rs` because it is a different kind of check: the guards in that
//! file compare the base config's *values* against pinned constants, this one refuses an entire class
//! of *file* that must not exist in a form that touches `plugins.updater`.
//!
//! Where it runs, stated precisely (the overclaim CPE-1873's reviewers kept having to correct):
//!
//! - **Here (`cargo test -p cpe-updater-verify`)** — `ci.yml`'s "updater-verify — clippy + test" step,
//!   on every push and PR to `main`, and `release-sidecar.yml`'s `verify-updater-pin` job, which gates
//!   the sidecar build/sign/publish matrix via `needs:` *before* anything is signed (preventive).
//! - **In the binary** — `verify-release-artifacts` runs the same library function on the directory
//!   holding its `--conf`, so `release.yml`'s tag-triggered `verify-published-manifest` job covers the
//!   plain channel's tag path, which no `#[test]` can reach (`ci.yml` has no `tags:` trigger). There it
//!   is *detective*, not preventive: that job is `needs: release`, so the draft release already
//!   exists — publishing is still a separate manual gate.
//! - **`src/lib/sidecarBundleResources.test.ts`** — the same derivation in TypeScript, so the frontend
//!   half of `verify-updater-pin` catches it too.
//!
//! The logic, its rationale, and the three rounds of enumeration it replaces live in
//! `src/platform_config_guard.rs`'s module doc. This file is only the wiring to the real directory.

use cpe_updater_verify::{platform_config_override_message, scan_for_platform_config_updater_overrides};

/// `<repo>/src-tauri` — `CARGO_MANIFEST_DIR` is `<repo>/crates/updater-verify`, so the app's config
/// root is two levels up. Same addressing every other guard/binary in this workspace uses.
fn repo_src_tauri_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("src-tauri")
}

/// CPE-1903 (supersedes CPE-1873 finding 6's three-filename version). Tauri merges a per-platform
/// config file AUTOMATICALLY, with no `--config` flag involved at all, and it does so for **fifteen**
/// filenames — three formats (`tauri.<t>.conf.json`, `tauri.<t>.conf.json5`, `Tauri.<t>.toml`) across
/// five `Target` variants — not the three `.json` names round 3 hardcoded. `.json5` and
/// `Tauri.windows.toml` were both demonstrated ingesting an attacker config through this repo's own
/// installed `@tauri-apps/cli` while every guard reported green.
///
/// So this does not name files. It lists the directory and classifies what is there — which also
/// closes the case leg for free: `read_dir` hands back the on-disk spelling, so a
/// `Tauri.Windows.Conf.json` is caught identically on NTFS and on the byte-exact `ubuntu-latest`
/// runner where `verify-updater-pin` actually executes. Round 3's `read_to_string(dir.join(name))` was
/// a *lookup*, and therefore silently blind on exactly that host.
#[test]
fn no_auto_merged_per_platform_config_overrides_the_updater_pin() {
    let dir = repo_src_tauri_dir();
    let hits = scan_for_platform_config_updater_overrides(&dir).unwrap_or_else(|e| {
        panic!(
            "SECURITY (CPE-1903): cannot list {} to check for auto-merged per-platform Tauri configs \
             ({e}). An unreadable config directory is not a clean one.",
            dir.display()
        )
    });
    assert!(hits.is_empty(), "{}", platform_config_override_message(&dir, &hits));
}
