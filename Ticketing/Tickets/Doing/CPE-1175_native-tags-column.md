---
id: CPE-1175
title: "Native Tags metadata column (opt-in, lazy per-path native read)"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-31
epic: CPE-717
---

## Summary
Part of the CPE-717 GUI remainder. Add a "Native Tags" column to the metadata-column catalog that lazily reads
native OS metadata (Finder tags / NTFS ADS / xattrs) per-path via the already-shipped `native_bridge` layer.
Opt-in, off by default, and **never on the hot `list_dir` path** (per the epic's decision) — read only when the
column is enabled, per visible row.

## Build
- Add a native-tags column to the generic column pipeline (`crates/server/src/column_extract.rs` +
  `column_cells.rs`), reusing the existing native-bridge read (`native_bridge::…` — the same path `native_tags_pull`
  uses). Cell returns the file's native tags (comma-joined) or blank on unsupported/absent (FAT, no xattr).
- Must degrade gracefully: `Unsupported`/no-metadata → empty cell, never an error that breaks the listing
  (preserve the skip-on-error guardrail).
- Regenerate `src/lib/bindings.gen.ts` (specta) if the column enum/struct changes.

## Acceptance Criteria
- [ ] `cargo test -p cpe-server` covers the column reader: returns tags for a file with native metadata; blank
      on unsupported/no-metadata; never panics/errors the listing.
- [ ] The column appears in the column-picker's available list (assertable via the `column-picker.smoke.ts`
      pattern).
- [ ] `cargo clippy --all-targets -D warnings` clean (both feature modes if applicable); bindings regenerated.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-717 GUI remainder). Backend-only; disjoint from the
  frontend tickets. Reads native bridge; does not touch the `nativeBridgeEnabled` frontend key.
