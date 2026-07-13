---
id: CPE-243
title: Right-click "Open" shows a folder icon even for files
type: Defect
status: Done
priority: Low
component: Frontend
estimate: 15m
created: 2026-07-12
closed: 2026-07-12
---

## Summary

In the middle pane's right-click context menu, the **Open** item hard-codes a
folder icon (`Icon name="folder"`). When the selected item is a file, "Open"
therefore shows a folder glyph, which is wrong/misleading. It should reflect the
selected item's actual type.

## Acceptance Criteria
- [ ] Right-clicking a file shows the file's own icon next to "Open" (not a folder).
- [ ] Right-clicking a folder still shows a folder icon.
- [ ] `npm run check` + tests pass.

## Resolution

*(Agent writes this when closing)*

## Work Log

*(Agent appends dated entries here)*

### filled
ContextMenu gained an `openIcon` prop; App passes the selected entry's own icon
(iconFor) so "Open" shows the file's icon for files and a folder for folders.
check 0/0. Ships in the next release.
