---
id: CPE-335
title: "AI Console as a native pop-out window (move/resize/frame)"
type: Feature
status: Done
priority: High
component: Frontend
created: 2026-07-13
closed: 2026-07-13
---

## Summary

The AI Console was an in-app modal overlay: it closed on click-away, couldn't be dragged,
resized, or moved around the screen, and had no window frame. A console needs to be a real,
persistent, movable window.

## What landed

`launchAiConsole` now opens the console in its **own Tauri `WebviewWindow`** (label
`ai-console`) loading the sidecar's loopback URL directly — native title bar (drag around
the whole screen), resize borders, min size, frame, and independence from the explorer's
focus. Reopening focuses the existing window (keeps the running session) instead of
respawning. Removed the in-app overlay + SidecarPane usage + its CSS.

Isolation is *better* than the sandboxed iframe: the window is a separate top-level window
on the loopback origin, and its label is in **no** capability, so it gets no Tauri API.
Clipboard/WebGL "just work" (127.0.0.1 is a secure context) without the iframe
allow-same-origin exception.

Also: terminal scrollbar now uses the standard `scrollbar-color`/`scrollbar-width` so
WebView2 shows an always-visible, non-overlay scrollbar (it was ignoring `::-webkit-`).

## Follow-up
- CPE-336: multiple consoles + docking (tabs / dock-manager). Needs backend multi-session
  (the host currently holds one AiConsoleState connection).
