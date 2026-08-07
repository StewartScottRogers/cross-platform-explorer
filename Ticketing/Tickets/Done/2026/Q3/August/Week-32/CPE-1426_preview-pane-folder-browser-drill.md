---
id: CPE-1426
title: "Preview pane as a folder browser — peek + click-to-drill cascading navigation"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-617
created: 2026-08-07
---

## Goal (user-directed, GUI session 2026-08-07)
When a **folder** is highlighted in the main file list, the **preview pane** shows that folder's contents as a
browsable list (a "peek" one level down, without navigating yet). Clicking a **subfolder** in the preview pane
makes the **main list dive into it**, and the preview re-points to the newly-highlighted folder — so you can walk
down a folder tree entirely by clicking subfolders in the preview pane (macOS Finder column-view / Miller-columns
feel, expressed across the main list + preview pane).

## Behaviour (confirmed pane mapping = main list + PREVIEW PANE)
1. Highlight (select) a folder in the main list → preview pane streams + shows that folder's contents (files +
   subfolders), reusing the existing streaming `list_dir` path.
2. Click a subfolder Y in the preview (which is showing highlighted folder X's contents) → the **main list
   navigates INTO X**, lands with **Y highlighted**, and the preview pane then shows **Y's contents**. Each click =
   exactly one level down; the peek is always maintained.
3. Back/Forward + breadcrumb + sidebar tree update exactly as a normal navigation into X would.

## Defaults chosen (override any of these)
- **Trigger:** any selection change to a folder (single-click select OR arrow-key highlight) drives the preview
  peek. Debounce rapid arrow-key changes (~150ms) so fast keyboard scrolling doesn't hammer the filesystem —
  only load after the selection settles.
- **Non-folder selection:** highlighting a *file* falls back to the existing file preview (unchanged). So the
  preview pane is: folder → folder-browser; file → file preview.
- **Clicking a FILE in the preview browser:** single-click previews it (shows its file preview in place);
  double-click opens it. (Alt: single-click could just select — TBD, low stakes.)
- **Mode:** not a separate mode — this is simply what the preview pane does when a folder is selected and the
  preview pane is visible. Independent of dual-pane commander mode (which stays as-is).
- **Empty / inaccessible folder:** preview shows an "empty folder" / permission note (reuse the skip-on-error
  listing behaviour), never an error dialog.
- **Sort/filter:** the preview-browser list uses the same sort as the main pane (or a sensible default); the main
  pane's file-type filter does NOT constrain the peek (you can see everything one level down). TBD.

## Notes / feasibility
Low-risk, mostly wiring: streaming directory listings already exist (peek paints instantly), and the preview pane
already renders per-selection content — this adds a "folder" content kind that is itself a mini clickable list
whose item-clicks drive `navigate()` on the main pane. Touches the preview-pane component + the main list's
selection→preview hookup + navigation. Add jsdom tests (folder-select → preview lists contents; preview-subfolder-
click → main pane navigates + re-peeks). Docs (CPE-579) if it adds a user-facing section.
