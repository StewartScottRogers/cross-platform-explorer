---
id: CPE-701
title: Backend rename_many command (collision-safe ordering + undo map)
type: feature
component: Backend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-699
estimate: 2-3h
---

## Summary
Child of CPE-699. A Tauri command that applies a list of `{ from, to }` rename pairs (produced by the
CPE-700 engine) safely: order the renames so no rename clobbers a not-yet-moved sibling, resolve
swaps/cycles via temporary names, report per-item success/failure instead of failing the whole batch
(matching the FS layer's skip-on-error), and return the inverse map so the operation is undoable.

## Scope
- `rename_many(dir, pairs) -> RenameManyResult` in `src-tauri/src/lib.rs`, registered in
  `generate_handler!`; capability if needed.
- Pure ordering helper (topological order; temp-name pass for cycles/swaps) factored out and
  **cargo-tested** independent of the filesystem — the safe headless part.
- Defensive re-validation of pairs (target collisions / illegal names) before touching disk.

## Acceptance Criteria
- [ ] Renaming a set including a swap (a→b, b→a) succeeds via temp names with no data loss.
- [ ] Per-item errors are reported, not fatal; partial results returned.
- [ ] Returns the inverse pairs for undo; `cargo test`/clippy (both feature modes) green.

## Notes
Prereq: CPE-700's `RenameResult` pair shape. The ordering/cycle logic is headless-testable; the live FS
apply is verified with CPE-702 attended.

## Work Log
