---
id: CPE-1408
title: "SidecarManager: a FAILED repair renders 'Repaired: Repair failed…' (reads as success)"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-862
created: 2026-08-07
---

## Problem (CPE-1406 / PR #682 spec observation)
`src/lib/components/SidecarManager.svelte`: `repair()`'s failure path sets `repairMsg[id] = $t("mgr.repairFailed")`
on a null result, but the template UNCONDITIONALLY prefixes `$t("mgr.repairDid")` ("Repaired") + `": "` regardless
of outcome. So a FAILED repair renders "Repaired: Repair failed — the platform may be off" — reads as a success
banner for a failure. Pinned by a test in `SidecarManager.test.ts` (CPE-1406).

## Fix direction
Track repair outcome (success vs failure) per id and render the message accordingly — only prefix "Repaired: " on
success; on failure show the failure message alone (and ideally a warn/error tone). Update the documenting test in
`SidecarManager.test.ts` to assert the corrected (non-misleading) failure text.
