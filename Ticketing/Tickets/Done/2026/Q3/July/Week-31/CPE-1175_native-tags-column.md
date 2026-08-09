---
id: CPE-1175
title: "Native Tags metadata column (opt-in, lazy per-path native read)"
type: feature
component: Backend
priority: medium
status: Done
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
- [x] `cargo test -p cpe-server` covers the column reader: returns tags for a file with native metadata; blank
      on unsupported/no-metadata; never panics/errors the listing.
- [x] The column appears in the column-picker's available list (assertable via the `column-picker.smoke.ts`
      pattern).
- [x] `cargo clippy --all-targets -D warnings` clean (both feature modes if applicable); bindings regenerated.

## Work Log
- 2026-07-31 — Filed by Foreman (sprint, epic CPE-717 GUI remainder). Backend-only; disjoint from the
  frontend tickets. Reads native bridge; does not touch the `nativeBridgeEnabled` frontend key.
- 2026-07-31 — Implemented by Worker. Added `MetaColumn::NativeTags` (id `native.tags`, label "Native
  Tags") to the catalog in `column_extract.rs`, applies-to-all like the CPE-1166 detectors. Added a
  read-only `native_bridge::read_native_tags(path)` — same low-level read `pull`/`pull_ctx` use
  (`native_meta::read` + the OS codec) but with **no `TagStore` involved**, so displaying the column
  never mutates the persisted tag store. `column_cells::stream_column_cells` special-cases the column to
  skip the 1 MiB header read entirely (native tags need no byte content) and calls
  `column_extract::native_tags_cell` to comma-join the tags, or `Empty` when there are none — covering
  no-metadata, unsupported filesystem, and missing/unreadable path, all via the existing
  skip-on-error contract (never panics, never fails the batch). `extract_column`'s bytes-only dispatcher
  keeps a `NativeTags => CellValue::Empty` arm purely so it stays exhaustive/never called directly.
  Regenerated `src/lib/bindings.gen.ts` (the `MetaColumn` specta enum gained the new variant + doc
  comment) — diff is the added `"NativeTags"` union member only.
  Verified: `cargo test -p cpe-server` (full suite) green — 77 unit tests in the touched modules plus the
  existing `parser_panic_safety`/`sample_fixtures` integration suites, including 5 new tests (2 in
  `native_bridge`, 2 in `column_extract`, 1 in `column_cells`) covering the happy path (tags read back
  comma-joined + sorted), the no-metadata/unsupported-filesystem degrade, and a missing path. `cargo
  clippy --all-targets -- -D warnings` clean in the default feature set, `--features index`, and
  `--features specta` (used for the bindings regen). No new dependencies.
