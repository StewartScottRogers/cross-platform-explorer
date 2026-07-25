---
id: CPE-648
title: Remove superseded client-side thumbnailing + doc the conflict chooser
type: chore
component: Frontend
priority: low
status: Done
tags: ready
epic: CPE-615
created: 2026-07-18
closed: 2026-07-18
---

## Summary
After CPE-643 moved icon-view thumbnails to the backend decode path, `src/lib/thumbnails.ts` (the old
client-side canvas thumbnailing, CPE-257) is orphaned — nothing imports it but its own test. Delete it.
Also add the CPE-624 conflict chooser to the in-app docs (self-maintaining docs).

## Acceptance Criteria
- [x] `thumbnails.ts` + its test removed; nothing else referenced them; `npm run check` clean, suite green.
- [x] `03-explorer.md` mentions the copy conflict chooser (Replace / Keep both / Skip).

## Work Log
2026-07-18 (dayshift) — Cleanup after the thumbnail merge + docs.
