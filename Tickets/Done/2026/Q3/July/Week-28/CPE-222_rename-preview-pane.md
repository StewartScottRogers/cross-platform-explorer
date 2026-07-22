---
id: CPE-222
title: Rename the right pane to "Preview Pane"
type: Task
status: Done
priority: Low
component: Frontend
estimate: 15m
created: 2026-07-12
closed: 2026-07-12
---

## Summary

Standardise the right column's name to "Preview Pane". Rename the wrapper class `rightpane` (and
`rightpane-toggle`) to `preview-pane` / `preview-pane-toggle`, update the CSS and the integration test
that queries it, and label the region.

## Acceptance Criteria

- [ ] Right-pane wrapper class is `preview-pane` (App.svelte + app.css); toggle is `preview-pane-toggle`
- [ ] The App integration test querying `.rightpane .details` is updated
- [ ] `npm run check` clean; full suite green; `vite build` clean

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

2026-07-12 — Requested rename of the right pane to "Preview Pane" (pairs with the File List Pane rename, CPE-221).

## Resolution

Renamed the right-pane wrapper class `rightpane` -> `preview-pane` and `rightpane-toggle` -> `preview-pane-toggle` (App.svelte + app.css), and updated the App integration test query. check + suite (200) green; build clean.
