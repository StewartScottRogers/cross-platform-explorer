---
id: CPE-1058
title: "Updater manifest + signature verification guard (retire manual auto-update sanity check, burndown #6)"
type: test
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-25
estimate: 4-6h
burndown: "MANUAL-TEST-BURNDOWN row #6 (auto-update flow)"
---

## ✅ User-approved 2026-07-25 (workshift)
User green-lit building this ("do it"): the new **test/release-only `minisign` dep** (NOT linked into the
shipped app) and the **`release.yml` guard step** are both approved. The `release.yml` step MUST degrade
cleanly (skip, not fail) when signing secrets are absent, so it never breaks the green pipeline on forks/PRs.

## Summary
Retire the CI-automatable part of manual-test burndown **#6** (auto-update flow), which today has **zero**
automated coverage. The updater is a one-line Tauri plugin (`tauri_plugin_updater` in `src-tauri/src/lib.rs`);
`release.yml` builds `latest.json` + per-artifact minisign `.sig` files via `tauri-action`
(`includeUpdaterJson: true`), signed with `TAURI_SIGNING_PRIVATE_KEY`. At runtime the plugin fetches
`latest.json`, compares versions, downloads the artifact, and **verifies its minisign signature against the
`pubkey` in `tauri.conf.json`** before swapping the binary. Most of what a human checks after a release is
*artifact correctness*, not GUI behaviour: malformed `latest.json`, a signature that won't verify against the
configured `pubkey`, a version mismatch, a wrong/missing URL — all checkable without a GUI or a running app.
Automate those; the only piece that stays attended is the actual in-place binary swap on each OS.

## Design (buildable)
A new **standalone crate** `crates/updater-verify` (matching the standalone-crate pattern, out of any
workspace, like `cpe-net`) exposing a pure:
```rust
pub fn verify_update_manifest(
    manifest_json: &str, pubkey_config_b64: &str, expected_version: &str,
    artifact: impl Fn(&str) -> Option<Vec<u8>>,
) -> Result<(), Vec<ManifestProblem>>
```
Asserts, mirroring the plugin: (1) `latest.json` parses + has the required shape (top-level `version`,
per-platform `url` + `signature`); (2) `version == expected_version`; (3) each platform's minisign
`signature` verifies against the configured pubkey over the artifact bytes.

**Minisign format note (saves the worker a dead end):** both the config `pubkey` and each `signature` are
**double-base64** as Tauri stores them — base64-decode, then parse the inner minisign public-key / `.sig`
file; verify over the raw artifact bytes. Use the **`minisign` crate** (same lib Tauri uses). Confine it to
this crate; do NOT add it to `src-tauri`.

**Slice A — hermetic unit tests** (run in the existing 3-OS `Server crates` CI): generate an ephemeral
keypair in-test, sign fixture bytes, hand-build a `latest.json`, assert valid → OK; tampered artifact →
reject; wrong key → reject; version mismatch → reject; malformed manifest → reject with a clear
`ManifestProblem`.

**Slice B — real-release guard** (runs on tag over the actually-built artifacts): a thin
`verify-release-artifacts` bin + a `release.yml` step after the `tauri-action` build that reads `pubkey` +
`version` from `tauri.conf.json`, locates the produced `latest.json` + `.sig`, runs `verify_update_manifest`,
and **fails the job** on a manifest that wouldn't verify. Degrade cleanly (skip, not fail) when
`TAURI_SIGNING_PRIVATE_KEY` is unset (mirror the `catalog` job's has=true/false skip).

**CI wiring:** add a `working-directory: crates/updater-verify` block (clippy `--all-targets -D warnings` +
`cargo test`) to the **`Server crates`** job in `.github/workflows/ci.yml` + its path to the rust-cache
`workspaces` list. Wire slice B into `release.yml`.

## Acceptance Criteria
- [ ] `crates/updater-verify` exposes `verify_update_manifest(...)`; hermetic unit tests cover valid,
      tampered-artifact, wrong-key, version-mismatch, malformed-manifest — green on the 3-OS `Server crates`
      matrix (slice A pin).
- [ ] Verification faithfully mirrors the plugin: `minisign` crate, double-base64 decode of `pubkey` +
      `signature`, verify over raw artifact bytes.
- [ ] A `release.yml` step runs `verify-release-artifacts` over the real `latest.json` + `.sig`, fails the
      release on a bad manifest, skips cleanly when signing secrets are absent. (Slice B — needs green-light.)
- [ ] `clippy --all-targets -D warnings` + `cargo test` clean for the new crate on all 3 OSes.
- [ ] The new crate is NOT a dependency of the shipped app (`src-tauri`).
- [ ] Burndown #6 → 🔧 on filing; when green, mark the *download/verify/version* portion ✅ (pinned by the
      `Server crates` job + the `release.yml` verify step) and narrow the residual manual note to the
      in-place binary swap only (still attended). Keep MVD at 7 with a logged sub-surface win unless the
      narrowed residual is judged small enough to drop to 6.

## Work Log
2026-07-25 (workshift, QA Architect) — Filed as the highest-leverage CI-automatable MVD pick this shift.
Rejected row #7 (remote network): `crates/net` already tests a real client over a real `127.0.0.1` TCP
socket doing a real listing (`loopback_browse_returns_entries` + WS + streaming + security matrix); the only
manual delta left is true multi-host, needing a container/second host — not cargo-testable without new infra.
Row #6 had zero coverage and its dominant failure modes are pure artifact correctness → automatable without a
GUI. One new dep (`minisign`, the lib Tauri's updater uses), confined to the non-shipped verify crate.
2026-07-25 (workshift, Foreman) — Held for a user green-light rather than built overnight: adds a dep +
touches `release.yml`. Burndown #6 flipped to 🔧 in progress.
