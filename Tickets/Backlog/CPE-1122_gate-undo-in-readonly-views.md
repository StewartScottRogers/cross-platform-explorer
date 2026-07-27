---
id: CPE-1122
title: "Gate undo() in read-only views (archive / smart-folder / replay) for 'read-only means read-only'"
type: enhancement
component: Frontend
priority: low
status: Backlog
tags: ready
created: 2026-07-26
epic: CPE-728
---

## Summary
Non-blocking observation from the CPE-1112 (#432) re-review: `undo()` (Ctrl+Z, `App.svelte`) is **not gated** by
`blockedInArchive()` / smart-folder / the new Replay-mode read-only guard. It is pre-existing behaviour,
identical across all the read-only views, and is **not** a data-loss / wrong-file hazard — it doesn't read the
`selection` or the displayed listing (no index-mismatch), and it only reverses the user's own prior explicit
operation. So it was correctly out of scope for the CPE-1112 data-loss fix. But if the team wants "read-only
means read-only" to be *absolute* in these views, undo should also be blocked while they're active.

## Design (tiny)
Add the same read-only predicate (`blockedInArchive()` returning true, or the `replayOverlayEntries !== null` /
archive / smart-folder checks) to the top of `undo()` (`App.svelte`), showing the standard read-only notice
instead of reversing an op while a read-only view is showing.

## Acceptance Criteria
- [ ] Ctrl+Z is a no-op (with the read-only notice) while in archive / smart-folder / replay-overlay view;
      undo works normally otherwise; `npm run check` clean; `npm test` green; no new deps.

## Work Log
2026-07-26 (workshift) — Filed from the CPE-1112 #432 re-review (Reviewer's non-blocking observation). Low-pri
consistency polish; not a data-loss path.
