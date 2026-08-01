---
id: CPE-1197
title: "Snapshot↔live per-file content diff in the restore flow"
type: feature
component: Multiple
priority: medium
status: Doing
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
- [ ] gui-smoke screenshot of the diff panel opened from a snapshot preview (spec modeled on
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
- **Still open (frontend half, separate pass):** the "Open diff" affordance on `CheckpointDialog` preview
  rows wiring into `DiffPeek.svelte`, plus the gui-smoke screenshot pin. Not moved to Done — only the
  backend half is complete.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-735). Backend half in the backend batch; frontend half
  (CheckpointDialog + DiffPeek reuse) can run parallel.
- 2026-07-31 — Backend half implemented on branch `cpe-1196-1198-snapshot-backend`, alongside CPE-1196/1198.
  `cargo test -p cpe-server` green (3 new `checkpoint_store::` tests: correct before/after for a changed
  file, clean errors on binary/oversize/unknown-path, refuses a traversal `rel_path`). Clippy clean
  (default, `index`, `specta`). `npm run check` green with the regenerated bindings. Remains in `Doing` —
  frontend `DiffPeek` wiring + gui-smoke pin still outstanding.
