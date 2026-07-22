---
id: CPE-687
title: Integration tests for the listing pipeline (show-hidden, sort)
type: test
component: Frontend
priority: low
status: Done
created: 2026-07-18
closed: 2026-07-18
---

## Summary
Dayshift safe work + CPE-676 safety net. The App integration suite exercised search/new-folder/rename/
preview/tabs/type-ahead but not the **show-hidden** filter or **sort direction** — both part of the
derivation pipeline that CPE-676's state-ownership move relocates. Added two integration tests so that
move (when done) is verified end-to-end through the real component tree.

## Resolution
Added to src/App.features.test.ts: "show hidden toggle" (hidden entries absent by default, revealed when
the file-list toolbar's Show-hidden checkbox is toggled — the `shown` filter) and "sort direction"
(listing reverses when Direction → descending — `visible`/`sortEntries`). Both drive the real app via the
file-list toolbar gear popover. Suite green.
