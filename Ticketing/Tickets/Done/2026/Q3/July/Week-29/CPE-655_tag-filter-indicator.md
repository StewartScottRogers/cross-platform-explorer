---
id: CPE-655
title: Active tag-filter indicator (with clear button)
type: feature
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-614
---

## Summary
Child of CPE-614. When a sidebar tag filter is active, a small bar above the file list shows the tag,
the match count, and a ✕ to clear it — so it's obvious the view is filtered and how to undo it (the
sidebar highlight was the only prior cue).

## Acceptance Criteria
- [x] A `.tag-filter-bar` renders above the list only when `selectedTag` is set (tag + count + clear).
- [x] Clearing it (✕, toggling the sidebar tag, or navigating) restores the full listing.
- [x] `npm run check` clean; suite green.

## Work Log
2026-07-18 (dayshift) — Final tags-UI polish: a clear, discoverable active-filter indicator.
