---
id: CPE-1285
title: "Disguised-file (extension-mismatch) tree sweep"
type: feature
component: cpe-server
priority: medium
status: Done
tags: ready
created: 2026-08-03
epic: CPE-1000
---

## Summary
The per-row `TypeMismatch` metadata column exists, but there is no tree-wide **sweep** that lists every file
whose content doesn't match its extension — the security-review complement (a `.jpg` that's really a PE, a
`.pdf` that's really a ZIP). Add a `cpe-server` scan adapter. Headless, cargo-tested.

## Build
- New module `crates/server/src/type_mismatch_scan.rs` (declare `pub mod type_mismatch_scan;` in
  `crates/server/src/lib.rs`). A pure `fn find_type_mismatches(root: &Path) -> MismatchReport` that walks
  `root` (skip-unreadable), reads a capped header (~64 bytes) per file, calls `file_type::mismatch(&bytes,
  ext)`, and collects each hit as `{ path, claimed_ext, detected_label, detected_ext }` plus `scanned` /
  `truncated`. Container-safe (a `.docx` ZIP must NOT be flagged — rely on `file_type::mismatch`'s existing
  container handling).
- `#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]` + `Serialize` on the result struct(s).
- **No command / no bindings here** — integration is CPE-1287. Pure adapter + tests only.
- No new dep; never panics. Benefits from CPE-1286's widened signature coverage but does not depend on it.

## Acceptance criteria
- A `.jpg` that is really a PE is flagged; a genuine `.png` is not; a `.docx` (ZIP container) is not.
- `cargo test -p cpe-server` covers those; `cargo clippy` clean both feature modes; no new dep.

## Notes
Template: `folder_similarity_scan.rs`. Epic CPE-1000. Streaming variant is a possible follow-up.

## Work Log
- 2026-08-03 — type_mismatch_scan merged (#585). Reviewer APPROVE, 64B header cap sufficient (max 16 needed), container-safe (real ZIP-as-docx test), 10/10 re-run, clippy clean. Non-blocking: no symlink-dir test; walks dot-dirs (deliberate for a security sweep).
