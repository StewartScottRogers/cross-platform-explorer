---
id: CPE-566
title: "Workbench — show old/new line numbers in the diff"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-505
estimate: 45m
created: 2026-07-17
closed: 2026-07-17
---

## Summary
The Workbench diff (CPE-526) shows changed lines with an add/del marker but **no line numbers**, so you
can't tell where in the file a change is or reference it. Add an old/new line-number gutter, like a normal
diff viewer.

## Acceptance Criteria
- [x] The diff parser tracks a 1-based `oldLine`/`newLine` per line, derived from the `@@` hunk header
      (context lines get both; adds get new; dels get old). Pure + unit-tested.
- [x] The Workbench renders old + new line-number columns before the change marker; blank where a side
      doesn't apply. Numbers aren't selectable (copying grabs code only).
- [x] `npm run check` clean; diff parser test covers the line numbers.

## Resolution
`diff.ts`: `DiffLine` gained optional `oldLine`/`newLine`; `parseDiff` seeds counters from the
`@@ -a[,x] +b[,y] @@` header and advances them (context → both, add → new, del → old). `WorkbenchView`
renders two muted, `user-select:none`, tabular-nums `.lno` columns (old, new) before the `+/−` marker.
`diff.test.ts` +1 (asserts the per-line numbers for the sample); full suite **602 pass / 62 files**;
`npm run check` 0/0. Ran the full suite before landing.

## Notes
Pure parser change (safe) + a contained render addition; numbers-only, no i18n. Epic CPE-505 (Workbench).
