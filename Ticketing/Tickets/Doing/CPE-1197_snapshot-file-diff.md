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
- [ ] `cargo test -p cpe-server`: backend diff fn returns correct before/after for a changed file; clean error
      on binary/oversize.
- [ ] gui-smoke screenshot of the diff panel opened from a snapshot preview (spec modeled on
      `checkpoint-restore.smoke.ts`); `npm run check` + `npm test` green.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-735). Backend half in the backend batch; frontend half
  (CheckpointDialog + DiffPeek reuse) can run parallel.
