---
id: CPE-1132
title: "Recent" middle-pane view doesn't drive the right (preview/detail) pane like normal views
type: Defect
status: Done
priority: Medium
component: Frontend
estimate:
created: 2026-07-29
closed: 2026-07-29
tags: [ready]
---

## Summary

In the main explorer, selecting an entry in the **middle pane** normally populates the **right pane**
(preview / details) for that entry. The **Recent** view is inconsistent: selecting an item in Recent does
**not** update the right pane the way every other middle-pane view (regular folders, other smart/saved
views) does. Recent should behave like all other middle-pane listings — clicking an item drives the right
pane to that item's preview/details.

## Environment

- OS: Windows 11
- App version: 0.57.38 (sidecar build)
- Area: main explorer — middle (list) pane ↔ right (preview/detail) pane wiring

## Steps to Reproduce

1. Open the explorer; select a normal folder → click an item in the middle pane → the right pane shows its
   preview/details. (Correct baseline.)
2. Switch the middle pane to the **Recent** view.
3. Click an item in the Recent list.

## Expected Behavior

The right pane updates to show the selected Recent item's preview/details — identical to selecting an item
in any other middle-pane view.

## Actual Behavior

The right pane does not update for Recent selections (it stays blank / unchanged), so Recent is the odd one
out among middle-pane views.

## Acceptance Criteria

- [x] Selecting an item in the Recent view drives the right pane exactly like a normal folder view.
- [x] Behaviour is consistent across the other special/smart views too (no regression).
- [x] Covered by a test where practical (the selection→preview wiring is unit-testable in the pane logic).

## Notes

Likely in the explorer pane ↔ preview wiring (`src/lib/components/ExplorerPane.svelte` + the right/preview
pane component) and the special-view handling (Recent appears to be a smart/saved-search view — cf.
`App.svelte`'s "This is a smart folder — a saved search view" path). Probable cause: the Recent/smart-view
code path emits a different (or no) selection event, or passes a synthetic entry the right pane can't
resolve to a real path for preview. Investigate the selection event + the entry's path shape in the Recent
view vs. a normal listing. Filed at the user's request 2026-07-29.

## Root Cause (confirmed)

Not a smart-folder issue — Home's Recent tab is a curated dashboard (`HomeView.svelte`), not a
`<FileList>`. The right pane (`App.svelte`'s preview-pane column) is driven entirely by
`selectedEntries`, which is owned/derived by `ExplorerPane.svelte` from `selection` indexed into
`visible` — and `visible` only exists when `<FileList>` is rendered. When `inHome`, `ExplorerPane`
renders `<HomeView>` INSTEAD of `<FileList>`, so `visible` is `[]` and `selectedEntries` is always
`[]`. On top of that, each Recent row fired `dispatch("openFile", r.path)` on **both** `on:click` and
`on:dblclick`, so a single click didn't even attempt to select — it opened the file outright, via
`ExplorerPane`'s `on:openFile` → `openRecent` → `App.svelte`'s `openRecent()`.

## Fix (display-only Home preview)

Because `selectedEntries` also feeds file OPERATIONS (delete/rename/copy/run/tags/…), the fix does
**not** inject Home items into it — that would make a merely-previewed Recent file an accidental op
target. Instead, a separate, read-only path:

1. `HomeView.svelte` — Recent file rows: `on:click` now dispatches a new `select: string` event
   (path only); `on:dblclick` is unchanged (`openFile`, still opens).
2. `ExplorerPane.svelte` — forwards it: `on:select={(e) => dispatch("homeSelect", e.detail)}` on
   `<HomeView>`; added `homeSelect: string` to its dispatcher type.
3. `App.svelte` — new `homePreview: DirEntry | null` state, set by `selectHomeEntry(path)` (handles
   `on:homeSelect` on the primary `<ExplorerPane>`) via `commands.entriesForPaths([path])` — the same
   stat-a-path-into-a-`DirEntry` command smart folders already use (CPE-667), so no extra
   `EntryInfo → DirEntry` field mapping was needed; it returns the real `DirEntry` shape the
   preview/details panes already read everywhere else, and self-heals (empty result) if the file has
   since moved/vanished. `PreviewPane`'s `entry` and `DetailsPane`'s `selected` now fall back to
   `homePreview` only when `selectedEntries` is empty. `homePreview` is cleared reactively
   (`$: if (!isHome || selectedEntries.length > 0) homePreview = null;`) the instant it would go
   stale — leaving Home, or a real `<FileList>` selection landing.

Favorites/Folders tabs and folder-row navigation were left untouched (ticket's focus is Recent).

## Work Log

- 2026-07-29 — Branch `cpe-1132-recent-right-pane` off `origin/main`. Verified root cause by tracing
  `App.svelte` → `ExplorerPane.svelte` → `HomeView.svelte`. Implemented the display-only
  `select`/`homeSelect`/`homePreview` path (see Fix above). Added unit tests: `HomeView.test.ts`
  (single click → `select`, not `openFile`; double click still → `openFile`) and
  `ExplorerPane.test.ts` (Home's `select` forwards as `homeSelect`). `npm run check`: 0 errors, 0
  warnings. `npx vitest run`: 121 files / 1316 tests, all passing (including `App.test.ts`,
  `App.features.test.ts`, `App.replayGuards.test.ts` — no regression to the normal
  selection→preview/op wiring). Opened PR against `main`; not merged. Live-GUI verification
  (actually clicking Recent in the running app and eyeballing the preview pane) is pending the
  Foreman.

## Resolution

Fixed via **PR #447** (merged to `main` as `5b862dfd`). Home's Recent tab is a `HomeView` dashboard, not a
`FileList`, so it never produced the `selectedEntries` that drive the right pane. Added a display-only
Home-preview path: a Recent single-click dispatches `select` → `ExplorerPane` forwards `homeSelect` →
`App.selectHomeEntry()` resolves the path to a real `DirEntry` via `commands.entriesForPaths([path])` and
sets `homePreview`; `PreviewPane`/`DetailsPane` fall back to it only when `selectedEntries` is empty, and it
clears reactively so it never leaks into the op-selection. Double-click still opens. npm check clean, vitest
1316 pass (+ new HomeView/ExplorerPane tests), Foreman-reviewed, blocking CI green.
