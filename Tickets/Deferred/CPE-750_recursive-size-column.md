---
id: CPE-750
title: "Size (recursive)" sortable folder-size column in details view
type: feature
component: Frontend
priority: medium
status: Open
tags: ready
created: 2026-07-19
epic: CPE-706
estimate: 3-4h
---

## Summary
Child of CPE-706. Add an opt-in **"Size (recursive)"** details-view column that shows each folder's
computed subtree size (files already show their own size). Fills on demand via the existing `dir_size`
command — lazily, only for visible rows — cached per path, and sortable. Answers "which folder here is
big?" inline, before opening the treemap.

## Scope
- A pickable column (reuse the columns model / CPE-707 if it lands first, else a dedicated toggle) that,
  when on, requests `dir_size` for each **visible** folder row and renders the result (spinner/"…" until
  it resolves).
- Cache results per path (invalidate on refresh / folder change); never block the initial listing paint
  (compute after first paint, coordinate with virtualization CPE-690 so only on-screen rows compute).
- Sort by the recursive size (folders by computed size, files by own size); stable when some are pending.

## Acceptance
- [ ] Turning the column on fills folder sizes lazily for visible rows without stalling the listing.
- [ ] Values cache and the column sorts correctly (pending rows sort last / stable).
- [ ] No cost when the column is off; no regression to open/scroll speed (CPE-688).

## Notes
Uses the existing `dir_size`. Independent of CPE-749 (which powers the treemap). Headless-testable for the
cache/sort logic; the on-demand fill wants a GUI glance.
