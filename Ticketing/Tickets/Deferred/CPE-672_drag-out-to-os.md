---
id: CPE-672
title: Drag files OUT to other OS applications
type: feature
component: Multiple
priority: medium
status: Deferred
tags: deferred-internal
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

## Work Log
2026-07-18 (nightshift) — SPIKE done. Findings:
- **Plugin available + viable:** `@crabnebula/tauri-plugin-drag` v2.1.0 (npm registry reachable) with the
  Rust counterpart `tauri-plugin-drag`. JS API: `startDrag({ item: paths, icon })`. Cross-platform
  (Windows/macOS/Linux), carries real file paths.
- **Impl shape:** add both deps, register the plugin in `lib.rs run()`, add a `drag:` capability
  permission, and call `startDrag(selection paths)` from FileList `onDragStart`.
- **Risk (why deferred, not shipped):** `startDrag` starts a *native* OS drag, which can pre-empt the
  existing internal **HTML5** drag-to-folder gesture — dragging a row could stop dropping onto internal
  folders/sidebar. Reconciling native-drag-out with HTML5-drag-internal needs a decision (e.g. native
  drag only after a hold/threshold, or only when the drag leaves the window) and **live cross-app
  verification** that can't be done headlessly.

## Deferred
deferred-on: attended GUI verification + a decision on native-vs-HTML5 drag coexistence (above). Nothing
external blocks it — it's pickable anytime, but wants a human at the keyboard to drag into another app and
confirm internal DnD isn't regressed. revisit-when: next attended session or a /run drag-test.
