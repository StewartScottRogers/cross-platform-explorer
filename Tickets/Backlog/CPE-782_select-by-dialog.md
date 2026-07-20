---
id: CPE-782
title: "Select by…" dialog + selection stability across sort/filter
type: feature
status: Open
priority: medium
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-711
estimate: 2-3h
---

## Summary
The "Select by…" UI for epic CPE-711: a dialog to build a criteria (reusing the CPE-774 condition kinds),
apply it via `selectMatching` (CPE-780) to set the selection, plus "select same type" and invert; and keep
the selection stable across sort/filter/refresh (remap by path).

## Acceptance Criteria
- [ ] Users select by extension/glob/size/date/isDir; results match `selectMatching`.
- [ ] "Select same type" and invert work; selection survives a re-sort/filter/refresh.
- [ ] `npm run check` + suite green; GUI-verified.

## Notes
Prereq: CPE-780. Attended GUI. Reuse the existing selection remap-by-path helper.
