---
id: CPE-1368
title: "OS file-drop onto a folder row inside an opened archive writes to a synthetic virtual path"
type: Bug
status: Done
priority: High
component: Frontend
tags: [ready]
epic: CPE-059
created: 2026-08-06
closed: 2026-08-06
---

## Problem (found by the drag-and-drop adversarial audit)

An archive browse-view is read-only (internal drag is blocked via `canDrag={!archive}`), but dropping
**OS files from Windows Explorer / the desktop onto a FOLDER row inside the opened archive** bypassed that:
the files were sent to a synthetic in-zip path (e.g. "docs") that the backend resolves to some unexpected
on-disk location, or surfaced a confusing error.

## Root cause

`importDroppedFiles` (App.svelte) computes the destination as
`folderUnderCursor(pos) || (isHome || archive || … ? "" : currentPath)`. `folderUnderCursor` reads
`[data-drop-path]` straight off the DOM, and archive folder rows render `data-drop-path={entry.path}` where
`entry.path` is a SYNTHETIC in-zip relative path (`archiveChildren`). So `folderUnderCursor` returns a
non-empty string and the `archive ? ""` fallback never fires. The internal-drag path was guarded; the OS
drop-in path read the DOM attribute directly and never checked `archive`. Replay mode had an explicit early
guard for the identical reason (overlay rows also carry `[data-drop-path]`); the archive case had none.

## Fix

Added an early read-only guard in `importDroppedFiles` — `if (archive) { showNotice(...); return; }` —
right beside the existing Replay guard, before the destination is computed. Removed the now-redundant
`archive` term from the `dest` fallback. Closes the OS drop-in hole; internal drag was already blocked.

## Verification

`npm run check` clean. The OS drop-in path is fired by a Tauri `onDragDropEvent` the test harness mocks as
a no-op (the sibling Replay drop guard isn't unit-tested either for the same reason), so this is a
self-evident read-only early-return verified by the audit + analogy to the tested Replay guard pattern.

## Related (filed separately from the same audit)

- Drag BUG 2 (dual-pane cross-pane DnD inert) — CPE-1371 (medium).
- Drag BUG 3 (cursor shows "move" for a cross-volume copy) — CPE-1372 (low, cosmetic).

## Work Log

- 2026-08-06 — Drag-drop audit surfaced this read-only bypass. Guarded at importDroppedFiles, mirroring
  the Replay guard. Bundles into the next build.
