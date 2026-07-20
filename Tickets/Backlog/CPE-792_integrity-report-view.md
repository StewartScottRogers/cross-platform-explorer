---
id: CPE-792
title: Integrity report view (acknowledge / rebaseline)
type: feature
status: Open
priority: low
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-737
estimate: 2-3h
---

## Summary
The alerts/report UI for epic CPE-737: show the CPE-790 report ("N files changed unexpectedly", with
corrupted/missing highlighted vs. legitimate edits), and let the user acknowledge changes or rebaseline.

## Acceptance Criteria
- [ ] Report lists corrupted/missing prominently, edited/new secondarily; clear counts.
- [ ] Acknowledge and rebaseline actions update the stored baseline; menus follow MENUS.md.
- [ ] check + suite green; GUI-verified.

## Notes
Prereq: CPE-790, CPE-791. Attended GUI.
