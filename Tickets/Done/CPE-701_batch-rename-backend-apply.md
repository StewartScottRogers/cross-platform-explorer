---
id: CPE-701
title: Backend rename_many command (collision-safe ordering + undo map)
type: feature
component: Backend
priority: medium
status: Done
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
- [x] Renaming a set including a swap (a→b, b→a) succeeds via temp names with no data loss.
- [x] Per-item errors are reported, not fatal; partial results returned.
- [x] Returns the inverse pairs for undo; `cargo test`/clippy (both feature modes) green.

## Notes
Prereq: CPE-700's `RenameResult` pair shape. The ordering/cycle logic is headless-testable; the live FS
apply is verified with CPE-702 attended.

The live FS apply turned out headless-verifiable too (tempdir cargo tests, like `find_files_by_name`), so
this landed fully tested unattended — only the GUI panel (CPE-702) remains attended.

## Work Log
2026-07-18 22:40 USMST (nightshift) — Implemented in `src-tauri/src/lib.rs`:
- `RenamePair`/`RenameManyResult`/`RenameError` types; a **pure** `plan_renames` that orders `{from,to}`
  pairs into concrete steps that never clobber a name a later step needs, breaking cycles/swaps with
  `.cpe-rename-tmp-N` temp names (greedy: do any rename whose target is free; else break a cycle).
- `#[tauri::command] rename_many(dir, pairs)`: pre-validates every target (name-only via `valid_entry_name`,
  non-empty, unique targets, no collision with an out-of-set sibling) and **refuses the whole batch**
  (renames nothing) on a validation problem; applies the plan; reports per-item FS errors non-fatally;
  returns inverse pairs for undo. Registered in `generate_handler!`.
- Tests (+6): pure `simulate_plan` proving shift-chain / swap / 3-cycle never clobber and leave no temp
  behind; on-disk `rename_many` swap (contents exchanged, no temp leak, undo returned); clobber refused
  (nothing moved); duplicate-target and path-separator rejected. `cargo test` 141 pass; clippy
  `--all-targets -D warnings` clean in default + `sidecar-platform`. No frontend touched.
Next: CPE-702 wires the GUI panel over CPE-700's engine + this command (attended GUI verify).
