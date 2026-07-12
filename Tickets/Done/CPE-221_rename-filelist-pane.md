---
id: CPE-221
title: Rename the middle pane to "File List Pane"
type: Task
status: Done
priority: Low
component: Frontend
estimate: 15m
created: 2026-07-12
closed: 2026-07-12
---

## Summary

Standardise the middle column's name to "File List Pane". Its container used the generic class
`content`; rename it to `filelist-pane` and label it accordingly, updating the CSS rule and the
reserved-class guard test.

## Acceptance Criteria

- [ ] The middle-pane container class is `filelist-pane` (App.svelte + app.css)
- [ ] The `FileList.test.ts` reserved-global-class guard reflects the new name
- [ ] `npm run check` clean; full suite green; `vite build` clean

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

2026-07-12 — Requested rename of the middle pane to "File List Pane".

## Resolution

Renamed the middle-pane container class from `content` to `filelist-pane` (App.svelte + app.css), added `role="region"` / `aria-label="File list"`, and updated the reserved-global-class guard in FileList.test.ts. check + suite (200) green; build clean.
