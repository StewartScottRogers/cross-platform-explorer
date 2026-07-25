---
id: CPE-237
title: "Open in browser" folder-context action does nothing useful for a bundler index.html
type: Bug
status: Done
priority: Medium
component: Frontend
estimate: 20m
created: 2026-07-12
closed: 2026-07-12
---

## Summary

The folder-context "Web page" provider (CPE-235) fires whenever a folder contains
`index.html` and offers "Open in browser" (openPath the file). For a dev project
(e.g. a Vite/webpack app) that `index.html` is a bundler ENTRY — an empty
`<div id="app">` plus `<script type="module" src="/src/main.ts">`. Opened as a
file:// URL it renders a blank page (the module can't resolve), so the action
appears to "not work". openPath itself is fine (it opens the default .html app).

Fix: don't claim the "Web page" context when the folder is clearly a build
project (has `package.json`), because its `index.html` isn't a viewable static
page. Real static sites (index.html without build tooling) still get the offer.

## Acceptance Criteria
- [ ] A folder with `index.html` AND `package.json` no longer shows "Web page".
- [ ] A folder with a standalone `index.html` (no package.json) still does.
- [ ] Other contexts (Git/Node/…) for the same folder are unaffected.
- [ ] `npm run check` + tests pass.

## Resolution

*(Agent writes this when closing)*

## Work Log

*(Agent appends dated entries here)*

### filled
Web provider now returns null when the folder also has package.json (a bundler
project), so "Open in browser" is no longer offered where its index.html would
render blank. Standalone static pages still get it. check 0/0. Ships in 0.10.2.
