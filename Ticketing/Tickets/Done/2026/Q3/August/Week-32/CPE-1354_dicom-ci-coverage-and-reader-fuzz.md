---
id: CPE-1354
title: "Harden the shipped readers: run dicom-thumb tests in CI + add rar/dicom/camera_raw to the panic-safety battery"
type: Task
status: Done
priority: High
component: Multiple
tags: [ready]
epic: CPE-219
created: 2026-08-05
closed: 2026-08-06
---

## Problem (two related gaps, both in code shipped THIS session)

**A — `dicom-thumb` tests never run in CI.** The `dicom` module is `#[cfg(feature = "dicom-thumb")]`
(`crates/server/src/lib.rs`), the feature is OFF by default (`crates/server/Cargo.toml`), but it SHIPS in
the production binary (`src-tauri/Cargo.toml` lists it in the `cpe-server` features). CI's "server — clippy +
test" job (`.github/workflows/ci.yml`, ~line 251-266) runs bare `cargo test`, `cargo test --features index`,
and `cargo test --features pdf-thumb,video-thumb` — but **never `dicom-thumb`** (`grep -rn dicom-thumb
.github/workflows/` → 0 hits). So the 8 `dicom.rs` tests — including the CPE-1353 YCbCr sign-bug regression
test and the CPE-1350/1352 decode tests — **have never executed in CI**. A future edit could silently
reintroduce the sign bug and CI would stay green. `gui-smoke.yml`'s `tauri build` compiles the feature but
runs no `cargo test`, so it's not equivalent.

**B — the 3 new path-based readers aren't in the panic-safety fuzz battery.** `crates/server/tests/
binary_data_preview_panic_safety.rs` batteries the `&str`-taking parsers (pe/midi/wasm/torrent/spreadsheet/
sqlite) against adversarial input (truncated-at-every-boundary, all-0xFF, seeded pseudo-random, overflowing
lengths). `rar::rar_entries`, `dicom::read_dicom_tags`/`read_dicom_image_data_url`, and
`camera_raw::read_raw_preview_data_url` all take `path: &str` over untrusted bytes and are NOT in it (nor in
`parser_panic_safety.rs`) — even though CPE-1338 added exactly this for `model_3d` this same session. RAR and
DICOM are hand-rolled binary-format walkers = the highest-risk category.

## Do

**A:** In `.github/workflows/ci.yml`, add `dicom-thumb` to the existing feature-gated clippy+test invocations
(the `--features pdf-thumb,video-thumb` lines) — e.g. `--features pdf-thumb,video-thumb,dicom-thumb` — so the
`dicom.rs` tests + clippy run on the 3-OS matrix. (Verify locally first that `cargo test --features
dicom-thumb` and `cargo clippy --all-targets --features dicom-thumb -- -D warnings` are already green — they
are as of this session — so this only ADDS coverage, doesn't turn CI red.)

**B:** Add `rar::rar_entries`, `camera_raw::read_raw_preview_data_url` (always available) and — gated with
`#[cfg(feature = "dicom-thumb")]` — `dicom::read_dicom_tags` + `read_dicom_image_data_url` to
`binary_data_preview_panic_safety.rs`, reusing the file's existing `tempfile`-based adversarial harness
pattern (write each mutated byte buffer to a temp file, call the reader, assert it returns without panicking).
Update the file's doc-comment enumeration. (The dicom cases only actually RUN once part A adds the feature to
CI — that's fine; they still run locally with the feature.)

## Acceptance criteria

- CI's server job runs the `dicom-thumb` feature (clippy + test) on all 3 OSes.
- The panic-safety battery covers rar/camera_raw (and dicom under the feature); malformed/truncated/random
  input for each returns cleanly (no panic/hang), verified by the harness.
- `cargo test` (default AND `--features dicom-thumb`) green; `cargo clippy --all-targets -- -D warnings` +
  `--features dicom-thumb` green. No new deps.

## Notes

Two files: `.github/workflows/ci.yml` + `crates/server/tests/binary_data_preview_panic_safety.rs` (no
collision). High value: closes a live regression-detection hole for code shipped this session + extends the
codebase's own proof layer to the new highest-risk parsers. Surfaced by the 2026-08-05 frontier scan.

## Work Log
- 2026-08-06 (workshift): PR #649 merged. (A) CI server job now runs --features ...,dicom-thumb (clippy+test) so the DICOM tests incl. the CPE-1353 YBR regression actually execute in CI (were invisible). (B) rar/camera_raw/dicom added to binary_data_preview_panic_safety.rs battery with structurally-real magic seeds. Reviewer APPROVE (seeds verified reaching real parse code) + UAT PASS (proved non-hollow via injected-panic). Validated: Server-crates-ubuntu green with dicom-thumb.
