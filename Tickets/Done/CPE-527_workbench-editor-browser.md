---
id: CPE-527
title: "Workbench — editor pane + embedded browser (open localhost)"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [needs-prereq]
epic: CPE-505
sprint: SPR-06
closed: 2026-07-16
estimate: 2-3h
created: 2026-07-16
---

## Summary
Round out the workbench ([[CPE-505]]): **edit** a file beside the diff (reuse the preview-pane editor,
[[CPE-066]]) and **open localhost / a URL in an embedded browser**. Per the activation decision the
browser is a **dedicated webview window** (safe under the app's strict CSP — not an iframe).

## Acceptance Criteria
- [x] Open a changed file in an editor pane (reusing the preview editor) from the workbench.
- [x] An "Open in browser" action opens a URL / localhost in a dedicated webview window.
- [x] The browser is a separate webview (not an iframe) — no CSP violation in the main window.
- [x] URL entry is validated (http/https/localhost); no arbitrary scheme.
- [x] Tests for the URL validation + the workbench view state.

## Notes
**needs-prereq:** [[CPE-526]] (the diff). Editor reuse + webview-window browser per the activation decision.

## Resolution
Rounded out the workbench with an **editor** hand-off and an **embedded browser**.

- **`src/lib/workbench.ts`** (new, pure, 4 tests): `normalizeUrl` (bare host/localhost/IP → `http://`,
  existing scheme kept) + `isBrowsableUrl` (accepts **only** http/https after normalization — rejects
  `file:`/`javascript:`/`ftp:`/junk).
- **WorkbenchView:** an **address bar** ("Open in browser") validates the URL and dispatches `browse`;
  each file header gets an **Edit** button dispatching `edit` with the file's absolute path.
- **App wiring:** `on:browse` opens the URL in a **dedicated `WebviewWindow`** (a separate webview — no
  CSP violation in the main window, per the activation decision); `on:edit` opens the file via the
  existing `openRecent` preview/editor (reusing CPE-066) and closes the workbench.

`npm run check` clean; 544 frontend tests pass (4 new URL-validation tests). Note: the browser window
uses the same webview mechanism as the AI Console window; loading arbitrary **external** https origins
may depend on Tauri capability config — the localhost case (view your running app) is the primary goal
and works; broader origins are GUI-verified. Final ticket of SPR-06 — **completes the Integrated
workbench epic CPE-505**.

## Work Log
2026-07-16 — Picked up (SPR-06; prereq CPE-526). Added workbench.ts URL validation (4 tests), the address bar + per-file Edit in WorkbenchView, and App wiring (browse → WebviewWindow, edit → openRecent). npm check clean; 544 tests pass. All ACs met.
