---
id: CPE-1402
title: "WatchRulesDialog: Add-rule button stays enabled for invalid conditions, then silently no-ops"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-737
created: 2026-08-07
---

## Problem (CPE-1400 / PR #676 spec observation)
In `src/lib/components/WatchRulesDialog.svelte`, the Add-rule button's `disabled` binding only checks
`!ruleName.trim() || pending.length === 0` — it never re-validates `buildCondition()`. So an invalid pending
condition (blank ext list, both-blank or non-numeric size bounds, `olderThan` with days non-numeric or `0`)
leaves the button ENABLED, and clicking it silently no-ops: no row is added and no error is shown. The user gets
no feedback that their rule was rejected. (Pinned as observed behavior by 4 tests in `WatchRulesDialog.test.ts`,
CPE-1400.)

## Fix direction
Make the button's `disabled` also reflect whether `buildCondition()` returns a valid condition (or show an inline
validation message on click), so invalid input either disables Add or surfaces a clear reason. Update the 4
documenting tests in `WatchRulesDialog.test.ts` to assert the corrected behavior (button disabled / error shown)
instead of the silent no-op.
