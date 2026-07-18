---
id: CPE-677
title: Dual-pane layout toggle + split view
type: feature
component: Frontend
priority: low
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-617
estimate: 3-4h
---

## Summary
Child of CPE-617. A toggle (View menu / palette) that splits the window into two `<ExplorerPane>`
instances side by side, with a visible active-pane indicator and Tab to switch focus; persist the layout
choice. Preview pane is hidden in dual mode for v1. Prereq: CPE-676.

## Acceptance Criteria
- [ ] A toggle switches single ⇄ dual pane; OFF by default; single-pane unchanged.
- [ ] Two independent panes render side by side; active pane is clearly indicated; Tab switches focus.
- [ ] Layout choice persists across sessions; `npm run check` + suite green.

## Work Log
