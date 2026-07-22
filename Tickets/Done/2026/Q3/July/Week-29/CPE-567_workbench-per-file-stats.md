---
id: CPE-567
title: "Workbench — per-file +added / −removed badge in the diff"
type: Feature
status: Done
priority: Low
component: Frontend
tags: [ready]
epic: CPE-505
estimate: 20m
created: 2026-07-17
closed: 2026-07-17
---

## Summary
The Workbench shows an overall `+added −removed · files` summary, but each file's header only shows its
name. Add a per-file `+N −M` badge so you can see the size of each file's change at a glance (the standard
diff-viewer affordance).

## Acceptance Criteria
- [x] A pure `fileStats(file)` in `diff.ts` returns that file's added/removed line totals. Unit-tested.
- [x] Each non-binary file header shows a `+added −removed` badge (green/red).
- [x] `npm run check` clean.

## Resolution
`diff.ts`: `fileStats(f)` (delegates to `diffStats([f])`). `WorkbenchView` computes `{@const fs =
fileStats(f)}` per file and renders a `+{added} −{removed}` badge (green `.fs-add` / red `.fs-del`,
tabular-nums) in the file header for non-binary files; `.file-name` now `flex:1` so the badge + Edit sit
at the right. `diff.test.ts` +1 (per-file stats for the sample). Full suite **603 pass / 62 files**;
`npm run check` 0/0. Ran the full suite before landing.

## Notes
Numbers-only, no i18n. Follows CPE-566 on the Workbench diff (epic CPE-505).
