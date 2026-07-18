---
id: CPE-670
title: Drop files IN from the OS
type: feature
component: Frontend
priority: high
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-661
estimate: 2-3h
---

## Summary
Accept files dragged from the desktop/Explorer onto the window via Tauri v2's
`getCurrentWebview().onDragDropEvent()`, copying them to the folder under the cursor (a folder row or
sidebar place) else the current folder, with a themed full-window drop overlay while dragging over.
Prereq: CPE-669 (shared target resolution).

## Acceptance Criteria
- [ ] Dropping OS files onto the window copies them to the folder under the cursor, else current folder.
- [ ] A themed drop overlay shows while OS files are dragged over; hidden otherwise.
- [ ] HOME / archive / read-only contexts handled sanely (no drop where it can't apply).
- [ ] `npm run check` + suite green; capability for the webview drag-drop event added if required.

## Work Log
