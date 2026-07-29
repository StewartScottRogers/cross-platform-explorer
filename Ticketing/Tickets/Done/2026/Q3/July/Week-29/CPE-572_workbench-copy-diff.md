---
id: CPE-572
title: "Workbench — copy a file's diff to the clipboard"
type: Feature
status: Done
priority: Low
component: Frontend
tags: [ready]
epic: CPE-505
estimate: 30m
created: 2026-07-17
closed: 2026-07-17
---

## Summary
Add a **Copy** button per file in the Workbench diff to grab that file's unified-diff text (to paste into
a review, chat, or `git apply`).

## Acceptance Criteria
- [x] Pure `toPatch(file)` reconstructs a file's unified-diff text (`diff --git` / `---` / `+++` / hunks
      with +/−/space). Unit-tested (exact output + round-trips through parseDiff).
- [x] Each non-binary file header has a Copy button (clipboard) with a brief ✓; doesn't toggle collapse.
- [x] `npm run check` clean; component test asserts the copied text.

## Resolution
`diff.ts`: `toPatch(f)` rebuilds the unified diff from the parsed model (omits optional index/mode lines —
faithful for viewing/sharing; re-parses to the same hunks). `WorkbenchView`: a `copyPatch(f)`
(`navigator.clipboard.writeText`, brief `copiedFile` ✓) + a `Copy` button in each non-binary file header
(`stopPropagation` so it doesn't fold the file). `diff.test.ts` +1 (exact + round-trip),
`WorkbenchView.test.ts` +1 (clipboard mock). Full suite **612 pass / 63 files**; `npm run check` 0/0.
Non-i18n. Epic CPE-505.
