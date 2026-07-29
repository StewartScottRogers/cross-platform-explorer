---
id: CPE-656
title: Batch tagging (apply tags to a multi-selection)
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-614
---

## Summary
Nightshift loop. Tagging worked on a single file; "Tags…" now applies to a **whole multi-selection**.
The editor detects batch mode (count > 1), starts empty, and on Apply **adds** the entered tags to
each selected file's existing set (union, non-destructive) and sets the chosen colour label on all
(leaving labels untouched if "none" is left selected).

## Acceptance Criteria
- [x] Context-menu "Tags…" shows for a multi-selection (moved out of the single-only block).
- [x] `TagEditor` takes `paths`/`count`; batch apply unions added tags per file + conditional label.
- [x] Single-file editing unchanged; heading shows "{count} items" (reused `status.items`, no new i18n).
- [x] `npm run check` clean; suite green.

## Work Log
2026-07-18 (nightshift) — Extended the tag editor + context menu to batch a multi-selection.
