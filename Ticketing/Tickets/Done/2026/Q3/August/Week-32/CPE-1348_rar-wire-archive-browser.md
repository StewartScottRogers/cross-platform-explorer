---
id: CPE-1348
title: "Wire RAR listing into the archive browser: .rar branch in read_archive_entries + ARCHIVE_EXTS (browse-only)"
type: Feature
status: Done
priority: Low
component: Multiple
tags: [ready]
epic: CPE-111
created: 2026-08-05
closed: 2026-08-05
---

## Goal

Make `.rar` archives actually browse in the app's existing archive preview, using the pure-Rust RAR
listing backend just landed (CPE-1347, `crate::rar::rar_entries`). This is the wiring half of epic CPE-111.
Fully headless — no new command, no bindings change, no attended visual (it reuses the existing
zip/tar/7z/iso archive-entry listing UI).

## Changes

1. **Backend** — `crates/server/src/archive.rs`, `read_archive_entries` (dispatches by extension ~line 136):
   add an `else if lower.ends_with(".rar")` branch that returns `crate::rar::rar_entries(path)`. Match the
   existing branch style. (Extraction is NOT supported — do NOT add `.rar` to `extract_archive_*`; listing
   only.)
2. **Frontend** — `src/lib/archiveExts.ts`: add `"rar"` to **`ARCHIVE_EXTS`** (the browsable set) so a
   `.rar` selection opens the archive browser (which calls `read_archive_entries`). Do **NOT** add it to
   `EXTRACT_EXTS` or `ZIP_FAMILY_EXTS` (we can list but not extract/repack RAR) — RAR is browse-only, like
   the reasoning that keeps `iso` out of `EXTRACT_EXTS`.
3. **Tests**:
   - Rust: a `read_archive_entries` test that a synthetic `.rar` fixture (reuse the RAR4/RAR5 byte-builders
     from `rar.rs`'s tests, or a minimal one) lists the expected entries via the dispatch path.
   - Frontend: extend `src/lib/archiveExts.test.ts` — assert `ARCHIVE_EXTS.has("rar")` is true AND
     `EXTRACT_EXTS.has("rar")` is false (browse-only invariant), mirroring the existing `iso` assertions.

## Acceptance criteria

- Selecting a `.rar` in the app lists its entries in the archive browser (backend dispatch verified by a
  cargo test; frontend recognition verified by the vitest).
- `.rar` is browse-only: not extractable/repackable (test-guarded).
- `cargo test` + `clippy --all-targets -D warnings` (both feature modes) green; `npm run check` clean +
  the JS/vitest suite green. No new deps. No bindings regen (read_archive_entries already exists).

## Notes

Small, self-contained, headless. Completes RAR to end-to-end-usable. RAW (CPE-1346) and DICOM (CPE-1345)
readers still need their own preview-provider wiring (separate follow-ups; those need new providers +
attended visual). Touches `archive.rs` + `archiveExts.ts` + the two test files.

## Work Log
- 2026-08-05 (sprint): PR #643 merged. .rar now browses in the existing archive UI via read_archive_entries dispatch → crate::rar::rar_entries; browse-only (ARCHIVE_EXTS, not EXTRACT_EXTS). Reviewer APPROVE + UAT PASS; cargo+clippy+npm check+vitest all green. RAR is now end-to-end usable.
