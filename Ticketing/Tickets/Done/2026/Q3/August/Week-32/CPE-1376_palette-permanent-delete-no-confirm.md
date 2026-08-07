---
id: CPE-1376
title: "Command palette 'Delete permanently' skips the confirmation gate"
type: Bug
status: Done
priority: Low
component: Frontend
tags: [ready]
epic: CPE-059
created: 2026-08-06
closed: 2026-08-06
---

## Problem (file-operations audit, Finding 2)

The palette command `file.deletePermanent` (App.svelte) ran `() => doDelete(true)` directly, bypassing
`askDelete`'s "Delete permanently?" modal. The keyboard path (`askDelete(event.shiftKey)`) and the context
menu (`askDelete(false)`) both confirm; only the palette invoked the irreversible permanent delete with no
confirmation. It targeted the correct `selectedEntries` (not mis-targeting) — just a missing confirm on an
unrecoverable action.

## Fix

Route the palette command through `askDelete(true)` (the same confirm modal as Shift+Delete). `file.delete`
(trash, recoverable) stays as `doDelete(false)` — no confirm expected there. `npm run check` clean.

## Work Log

- 2026-08-06 — One-line fix aligning the palette permanent-delete with the already-confirmed keyboard/
  context-menu paths. Shipped in v0.57.58.
