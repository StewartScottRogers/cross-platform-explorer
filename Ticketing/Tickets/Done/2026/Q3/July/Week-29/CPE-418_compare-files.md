---
id: CPE-418
title: "Compare two files (are they identical?)"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [ready]
estimate: 1h
created: 2026-07-15
closed: 2026-07-15
---

## Summary
Select exactly two files → **Compare** → a notice says whether they are byte-for-byte identical.
Useful for de-duplication and confirming a copy. Backend streams both files with an early exit on the
first differing byte (cheaper + collision-free vs hashing both). Nightshift research loop 7.

## Acceptance Criteria
- [x] `files_identical(a, b)` returns true/false; different sizes short-circuit to false; a folder or
      missing path → Err, never a panic.
- [x] Context menu shows **Compare** only when exactly two files (no folders) are selected.
- [x] Result surfaced as a notice ("Files are identical" / "Files differ").
- [x] Backend unit tests (identical / different size / different content); `npm run check` clean; suite green.

## Work Log
2026-07-15 — Nightshift loop 7. Estimate 1h. Backend streaming compare + test; ContextMenu `comparable`
prop + row; App `compare` action → notice. Verify headlessly.

2026-07-15 — Done. Backend `files_identical(a,b)` (size short-circuit → streaming byte compare, early exit; dir/missing→Err) +test. `ContextMenu` gains a `comparable` prop + Compare row (+2 tests); `App.svelte` passes `comparable` (exactly 2 files, no folders), `compare` action → `compareFiles()` → identical/differ notice. `cargo test` + clippy clean, `npm run check` 0/0, `npm test` **405 + ContextMenu 4** green.

## Resolution
Streaming compare is O(size) with early exit and no hash-collision risk. Surfaced as a lightweight notice; a full side-by-side diff is out of scope.
