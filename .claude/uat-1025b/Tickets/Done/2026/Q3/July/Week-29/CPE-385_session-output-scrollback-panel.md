---
id: CPE-385
title: "AI Console: scrollable 'session output' panel with a draggable scrollbar"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
created: 2026-07-14
closed: 2026-07-14
---

## Summary

Full-screen TUI agents (Claude Code) own the terminal + their own scrolling, so the terminal's
custom scrollbar is (correctly) hidden — there's no linear scrollback (CPE-337). Users still want a
draggable way to review everything the session produced. Add a **read-only "Session output" panel**:
fetch the session's captured ring buffer, strip alternate-screen sequences so it accumulates in a
scrollable main buffer, render it in a read-only xterm, and drive the existing custom draggable
scrollbar (`.sb-thumb`).

## Acceptance Criteria

- [x] `GET /api/session/{id}/output` returns the session's captured bytes (base64, raw-faithful).
- [x] A "Scrollback"/output button per session opens an overlay with a read-only terminal replay.
- [x] The overlay reuses the custom draggable scrollbar (works in WebView2, unlike native).
- [x] Panel has the standard visible border; Esc/backdrop closes; the replay term is disposed on close.

## Notes
Requested by the user after confirming Claude Code's own mouse-wheel scroll works (so this is the
"see the whole captured log" affordance, not a scrollbar bug). Launcher UI — visual QA by eyeball
(rebuild the sidecar preview).

## Work Log
2026-07-14 — Building option 2 (scrollable output panel) per user request.
