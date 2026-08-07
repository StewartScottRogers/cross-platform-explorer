---
id: CPE-1373
title: "Ctrl+A / invert / select-all-of-type yank the viewport to the last row"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-059
created: 2026-08-06
---

## Problem (selection-model audit, Finding 3)

`selectAll` sets `lead = count-1`; `selectIndices`/`invertSelection` set `lead` to the max index; the lead
is then force-scrolled into view. So pressing Ctrl+A at the top of a long folder immediately scrolls to the
BOTTOM (Explorer/Finder keep the scroll position). Same jump on Invert / Select-all-of-type.

## Fix direction

For bulk selections (select-all/invert/select-of-type), don't move the lead to a far row / don't force a
scroll — keep the current scroll position (or set lead without triggering ensureLeadVisible). Preserve
follow-the-lead scroll only for single-item arrow/click navigation.
