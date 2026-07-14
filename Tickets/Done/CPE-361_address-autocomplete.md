---
id: CPE-361
title: "Address bar autocomplete from recent folders"
type: Feature
status: Done
closed: 2026-07-14
priority: Low
component: Frontend
created: 2026-07-14
---

## Summary

When typing a path in the address bar (Ctrl+L / Alt+D), suggest recently-visited folders. Reuses
the recent-folders MRU (CPE-342) via a native `<datalist>` — the browser filters as you type,
zero extra logic.

## Design (frontend)
- `NavToolbar.svelte`: the path-edit input gets `list` + a `<datalist>` of recent folder paths.
- `App.svelte`: pass `recentPaths={recentFolders.map(r => r.path)}` to NavToolbar.

## Acceptance
- Focusing the address bar and typing shows matching recent folders as suggestions; picking one
  navigates there. `npm run check` + `npm test` green.

## Work Log
2026-07-14 — Filed during Nightshift (loop 7). Small, high-utility reuse of CPE-342 data.

2026-07-14 — Implemented on branch `CPE-361-address-autocomplete`.
- `NavToolbar.svelte`: `recentPaths` prop; the path-edit input gets `list="recent-paths"` +
  a `<datalist>` of those paths (browser filters as you type).
- `App.svelte`: passes `recentFolders.map(r => r.path)`.
- `NavToolbar.test.ts` (new): asserts the recent paths render as datalist options in edit mode.
- `npm run check` 0 errors; suite 323 pass; `npm run build` ok.
