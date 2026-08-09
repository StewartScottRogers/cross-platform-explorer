---
id: CPE-1058
title: "Updater manifest + signature verification guard (retire manual auto-update sanity check, burndown #6)"
type: test
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-25
estimate: 4-6h
burndown: "MANUAL-TEST-BURNDOWN row #6 (auto-update flow)"
---

## ✅ User-approved 2026-07-25 (sprint)
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
2026-07-25 (sprint, QA Architect) — Filed as the highest-leverage CI-automatable MVD pick this shift.
Rejected row #7 (remote network): `crates/net` already tests a real client over a real `127.0.0.1` TCP
socket doing a real listing (`loopback_browse_returns_entries` + WS + streaming + security matrix); the only
manual delta left is true multi-host, needing a container/second host — not cargo-testable without new infra.
Row #6 had zero coverage and its dominant failure modes are pure artifact correctness → automatable without a
GUI. One new dep (`minisign`, the lib Tauri's updater uses), confined to the non-shipped verify crate.
2026-07-25 (sprint, Foreman) — Held for a user green-light rather than built overnight: adds a dep +
touches `release.yml`. Burndown #6 flipped to 🔧 in progress.
2026-07-25 (sprint, Worker) — Built both slices on branch `cpe-1058-updater-verify`.
- **Crate:** new standalone `crates/updater-verify` (`cpe-updater-verify`), out of any workspace (no root
  `Cargo.toml`; `crates/.gitignore` keeps `target/` out), **NOT** a dependency of `src-tauri` (grep-confirmed
  clean). One crypto dep: `minisign = "0.9"` (v0.9.1 resolved) — the same lib Tauri uses; confined here.
- **API note (differs slightly from the ticket's sketch, resolved from docs.rs):** in `minisign` 0.9,
  `verify` and `sign` are **free functions**, not `PublicKey` methods. Used
  `minisign::sign(Some(&pk), &sk, Cursor, trusted, untrusted) -> SignatureBox` and
  `minisign::verify(&pk, &sig_box, Cursor::new(bytes), quiet=true, output=false, allow_legacy=true)`. The
  double-base64 decode is exactly as the ticket described: config `pubkey` → base64-decode → 2-line minisign
  public-key file → `PublicKeyBox::from_string(..).into_public_key()`; each `signature` → base64-decode →
  `.sig` file text → `SignatureBox::from_string(..)`. Verified the config pubkey in `tauri.conf.json` really
  does decode to `untrusted comment: minisign public key: 521E574F68E2561A\nRW…`. `allow_legacy=true` so both
  prehashed (Tauri's form) and legacy ed25519 sigs verify.
- **Assumption (logged):** the loader closure returning `None` for a URL means "artifact not present here" →
  that platform's *crypto* check is **skipped** (its shape is still validated), so the per-OS release matrix
  can each verify only the platforms they built while a merged `latest.json` names others. The bin guards
  against a vacuous pass: if the manifest has platforms but **zero** matched a local artifact, it fails.
- **Slice A:** 13 hermetic unit tests (valid / tampered-artifact / wrong-key / version-mismatch /
  unparseable / missing-version / missing-signature / missing-url / no-platforms / array-form / bad-pubkey /
  bad-signature-encoding / missing-artifact-skips-crypto). In-test ephemeral keypair encoded into the real
  double-base64 shape so the real decode path is exercised.
- **Slice B:** `verify-release-artifacts` bin reads `pubkey`+`version` from `tauri.conf.json`, walks a search
  dir for `latest.json` + artifacts (match by URL basename), runs `verify_update_manifest`, exits non-zero on
  a bad manifest. Plus 3 integration tests (`tests/release_guard.rs`) that scaffold a real config+manifest+
  artifact in a tempdir and run the compiled bin: valid → exit 0, tampered → non-zero, version-mismatch →
  non-zero. `release.yml` step added AFTER the tauri-action build, guarded by a `has=true/false` check on
  `TAURI_SIGNING_PRIVATE_KEY` (mirrors the `catalog` job) so it **skips cleanly** on forks/PRs/unsigned repos.
- **CI:** added an `updater-verify` `working-directory` block (clippy `--all-targets -D warnings` + `cargo
  test`) to the 3-OS `Server crates` job + `./crates/updater-verify -> target` to the rust-cache list.
- **Verified locally (Windows):** `cargo clippy --all-targets -- -D warnings` clean; `cargo test` = 16
  passed / 0 failed (13 unit + 3 integration). No Defender os-error-225. Burndown #6 note narrowed to the
  in-place binary swap only; **MVD count left at 7 for the Foreman to call.**
