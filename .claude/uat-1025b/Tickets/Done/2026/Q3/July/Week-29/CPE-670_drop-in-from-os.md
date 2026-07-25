---
id: CPE-670
title: Drop files IN from the OS
type: feature
component: Frontend
priority: high
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-661
estimate: 2-3h
---

## Summary
Accept files dragged from the desktop/Explorer onto the window via Tauri v2's
`getCurrentWebview().onDragDropEvent()`, copying them to the folder under the cursor (a folder row or
sidebar place) else the current folder, with a themed full-window drop overlay while dragging over.
Prereq: CPE-669 (shared target resolution).

## Acceptance Criteria
- [x] Dropping OS files onto the window copies them to the folder under the cursor, else current folder.
- [x] A themed drop overlay shows while OS files are dragged over; hidden otherwise.
- [x] HOME / archive / read-only contexts handled sanely (no drop where it can't apply).
- [x] `npm run check` + suite green; capability for the webview drag-drop event added if required.

## Work Log

## Work Log
2026-07-18 (nightshift) — Picked up (prereq CPE-669 landed). No questions; best-guess.

## Resolution
Registered `getCurrentWebview().onDragDropEvent` in App.svelte onMount (guarded in try/catch so the absent
API in jsdom/non-Tauri can't break startup; unlistened on destroy). `enter`/`over` show a themed
full-window drop overlay (`.os-drop-overlay`, accent dashed border + card); `drop` copies the OS file
paths via `copy_entries` — always a COPY so external originals stay put — into the folder under the cursor
(hit-tested via a new `data-drop-path` attribute on FileList folder rows + Sidebar places, physical→CSS
px by devicePixelRatio) else the current folder; HOME/archive/smart-folder with no folder under cursor
shows a notice instead. 3 i18n keys ×12 locales (dnd.dropToImport/openFolderToImport + itemCount from 669).
check clean; suite green (666); bundle clean.

No plugin needed (Tauri v2 core). GUI-critical: the actual drag-from-Explorer + cursor hit-test can only
be confirmed live — recommended on /run. Files: src/App.svelte, src/lib/components/FileList.svelte,
Sidebar.svelte, src/lib/i18n.ts.
