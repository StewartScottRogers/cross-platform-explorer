---
id: CPE-672
title: Drag files OUT to other OS applications
type: feature
component: Multiple
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-661
estimate: 3-4h
---

## Summary
Let the user drag a file (or multi-selection) out of the app and drop it into another OS application.
Tauri v2 core has no drag-out; this needs a plugin (`tauri-plugin-drag` / equivalent). **Spike first:**
confirm cross-platform (Windows/macOS/Linux) viability, licensing, and that it carries real file paths;
if viable, wire a native `startDrag` from the file rows carrying the current selection's paths. If a
platform is unsupported, gate gracefully. Prereq: CPE-669.

## Acceptance Criteria
- [ ] Spike documented: chosen plugin/API + per-OS support + any gaps (in the Work Log).
- [ ] Where supported, dragging a selection starts a native OS drag carrying its real file paths; a drop
      into another app copies the files there.
- [ ] Unsupported platforms degrade gracefully (internal DnD unaffected); clippy clean both modes.

## Work Log
