---
id: CPE-527
title: "Workbench — editor pane + embedded browser (open localhost)"
type: Feature
status: Open
priority: Medium
component: Frontend
tags: [needs-prereq]
epic: CPE-505
sprint: SPR-06
estimate: 2-3h
created: 2026-07-16
---

## Summary
Round out the workbench ([[CPE-505]]): **edit** a file beside the diff (reuse the preview-pane editor,
[[CPE-066]]) and **open localhost / a URL in an embedded browser**. Per the activation decision the
browser is a **dedicated webview window** (safe under the app's strict CSP — not an iframe).

## Acceptance Criteria
- [ ] Open a changed file in an editor pane (reusing the preview editor) from the workbench.
- [ ] An "Open in browser" action opens a URL / localhost in a dedicated webview window.
- [ ] The browser is a separate webview (not an iframe) — no CSP violation in the main window.
- [ ] URL entry is validated (http/https/localhost); no arbitrary scheme.
- [ ] Tests for the URL validation + the workbench view state.

## Notes
**needs-prereq:** [[CPE-526]] (the diff). Editor reuse + webview-window browser per the activation decision.
