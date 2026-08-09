---
id: CPE-1197
title: "Snapshot↔live per-file content diff in the restore flow"
type: feature
component: Multiple
priority: medium
status: Done
tags: ready
created: 2026-07-31
epic: CPE-735
---

## Summary
Part of CPE-735. The folder restore flow (CheckpointDialog) lists changed files but has no per-file diff. Add a
backend diff command + an "Open diff" affordance reusing the existing `DiffPeek.svelte` (the same component
AgentTimeline uses). Completes the DoD's "browse the timeline and restore" with a diff.

## Build
- **Backend:** command `(root, manifest_id, rel_path) → { before: blob bytes/text, after: live file text }` in
  `checkpoint_store.rs`; graceful binary/oversize handling (mirror the existing preview-skip semantics). Thin
  command in `lib.rs` + `bindings.gen.ts` regen. (Backend half goes with the CPE-1196/1198 backend batch.)
- **Frontend:** an "Open diff" affordance on CheckpointDialog preview rows → reuse `src/lib/components/DiffPeek.svelte`.

## Acceptance Criteria
- [x] `cargo test -p cpe-server`: backend diff fn returns correct before/after for a changed file; clean error
      on binary/oversize.
- [x] gui-smoke screenshot of the diff panel opened from a snapshot preview (spec modeled on
      `checkpoint-restore.smoke.ts`); `npm run check` + `npm test` green.

## Build — backend half (landed)
- `checkpoint_store.rs`: `checkpoint_diff_file(ctx, root, manifest_id, rel_path) -> Result<FileDiff, String>`
  — reads the checkpointed blob straight from the store (bypassing a full `snapshot_capture::restore`) as
  `before`, the live file under `root` as `after`. `rel_path` is resolved safely via
  `revert_engine::safe_target` (made `pub(crate)`, reused rather than duplicated) so a traversal-shaped path
  can't escape `root`. Errors cleanly (never truncates/replaces) when: the path isn't in the checkpoint,
  either side exceeds a 5 MiB cap, or either side isn't valid UTF-8 — mirrors `read_file_text`'s
  error-rather-than-truncate preview-skip convention in `src-tauri/src/lib.rs`.
- Thin Tauri dispatcher `checkpoint_diff_file` in `src-tauri/src/lib.rs`, registered in `generate_handler!`
  + `collect_commands!`; `bindings.gen.ts` regenerated (`FileDiff` type).
- Frontend half (below) completed the wiring + gui-smoke pin — both halves are now Done.

## Work Log
- 2026-07-31 — Filed by Foreman (sprint, epic CPE-735). Backend half in the backend batch; frontend half
  (CheckpointDialog + DiffPeek reuse) can run parallel.
- 2026-07-31 — Backend half implemented on branch `cpe-1196-1198-snapshot-backend`, alongside CPE-1196/1198.
  `cargo test -p cpe-server` green (3 new `checkpoint_store::` tests: correct before/after for a changed
  file, clean errors on binary/oversize/unknown-path, refuses a traversal `rel_path`). Clippy clean
  (default, `index`, `specta`). `npm run check` green with the regenerated bindings. Remains in `Doing` —
  frontend `DiffPeek` wiring + gui-smoke pin still outstanding.
- 2026-07-31 — Frontend half implemented on branch `cpe-1197fe-snapshot-diff-ui`. `CheckpointDialog.svelte`'s
  drift-list rows (the preview's changed-file list) each get an "Open diff" button; clicking it calls
  `commands.checkpointDiffFile(root, manifestId, relPath)` and renders the returned `before`/`after` through
  the reused `DiffPeek.svelte` (same component `AgentTimeline` uses — not rebuilt) in an inline per-row panel
  that toggles closed on a second click. A `checkpointDiffFile` rejection (binary/oversize/unknown-path) is
  caught and shown as a small `diff-error` notice in the same panel instead of throwing/crashing the dialog.
  Added a `CheckpointDialog.test.ts` suite (2 new tests): asserts the exact `checkpoint_diff_file` invoke args
  and that `DiffPeek`'s rendered output contains both the before and after text, plus a rejected-invoke case
  asserting the notice renders (not a crash) and no exception escapes. `npm run check`: 0 errors. `npm test`:
  139 files / 1558 tests green (includes the 9 `CheckpointDialog.test.ts` tests, up from 7). Added
  `gui-smoke/specs/snapshot-diff.smoke.ts` (modeled on `checkpoint-restore.smoke.ts`): opens the real dialog
  via the Command Palette ("Checkpoint & rollback…"), creates a genuine checkpoint over a seeded file through
  the dialog's own Create button, stages drift by rewriting the file after the checkpoint, previews the
  revert, clicks the seeded file's "Open diff" row, and asserts the diff panel renders both the checkpoint's
  and the live file's text before `snap("snapshot-diff")`. `cd gui-smoke && npm run typecheck`: clean (after
  `npm install` in `gui-smoke/`, whose `node_modules` wasn't present in this worktree). The live headless run
  of the new spec itself is CI's job, not verified locally here. Both halves of CPE-1197 are now complete —
  moved to Done.
