---
id: CPE-1407
title: "ColorRulesDialog: Add-rule button has no disabled binding — invalid condition silently no-ops"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-707
created: 2026-08-07
---

## Problem (CPE-1405 / PR #681 spec observation — same class as CPE-1402)
`src/lib/components/ColorRulesDialog.svelte`'s `add-btn` has NO `disabled` binding at all. With an invalid/empty
condition (blank extensions, blank glob, both size bounds blank, NaN size bound, zero/non-numeric days), the Add
button stays clickable and a click silently no-ops (`buildCondition()` returns null, `add()` returns early — no
rule, no `change` dispatch, no visual signal). Identical bug class to WatchRulesDialog, fixed in CPE-1402; this
dialog was never covered. Pinned by 5 tests in `ColorRulesDialog.test.ts` (CPE-1405).

## Fix direction
Apply the SAME fix as CPE-1402: add a reactive `$: condition = (inlined builder switch)` + `$: validCondition =
condition !== null`, gate the Add button `disabled` on `!validCondition` (and reuse the reactive `condition` in
`add()`, removing the duplicate build). Update the 5 documenting tests in `ColorRulesDialog.test.ts` to assert the
button is DISABLED for invalid input instead of the silent no-op.
