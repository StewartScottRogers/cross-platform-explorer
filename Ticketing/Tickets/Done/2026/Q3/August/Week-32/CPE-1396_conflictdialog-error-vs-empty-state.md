---
id: CPE-1396
title: "ConflictDialog: on a load failure, the 'nothing to resolve' empty-state renders alongside the error"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-715
created: 2026-08-07
---

## Problem (CPE-1391 / PR #669 spec observation)
In `src/lib/components/ConflictDialog.svelte`, when `forge_conflict_state` REJECTS (load failure), `operation`
and `files` stay at their initial defaults (`"none"` / `[]`), so the "No conflicts — nothing to resolve."
empty-state panel renders at the same time as the real error text in the status line. The user sees a
contradictory "nothing to resolve" message next to an actual error. Command wiring is correct; this is a
UI-state bug (error and empty-state aren't mutually exclusive).

## Fix direction
Track a distinct `loadError` (or `loaded`) state so the empty-state panel renders ONLY on a successful load
with zero conflicts, and the error state suppresses the "nothing to resolve" panel. Extend
`ConflictDialog.test.ts` (added in CPE-1391) to assert the empty-state is NOT shown on a load error.
