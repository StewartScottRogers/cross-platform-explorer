---
id: CPE-809
title: Diagnostics overlay crashes on duplicate {#each} key (same-ms OS calls)
type: bug
status: Done
priority: high
component: Frontend
tags: ready
created: 2026-07-20
closed: 2026-07-20
epic: CPE-758
estimate: 15m
---

## Summary
Found during CPE-776 GUI verification in the running dev app. With Diagnostics mode on (CPE-758), the
overlay's recent-calls list keyed its `{#each}` on `c.at + c.cmd` (call timestamp + command name). Two OS
calls of the same command in the **same millisecond** (e.g. two `disk_space` reads) produce an identical
key, and Svelte throws `Cannot have duplicate keys in a keyed each`. The throw happens inside Svelte's
flush/update cycle, which **aborts the whole flush** — so with diagnostics on, the app silently stops
applying reactive updates (navigation renders nothing, etc.). Reproduced live: `EXCEPTION … duplicates
'1784571927036disk_space'` from `DiagnosticsOverlay.svelte`, and folder navigation produced 0 rows until
the key was fixed.

## Acceptance Criteria
- [x] The recent-calls `{#each}` key is unique even for same-timestamp, same-command calls.
- [x] With diagnostics on, navigation/reactive updates work (verified live: 36 rows render after the fix).
- [x] `npm run check` + suite green.

## Resolution
Changed the key from `(c.at + c.cmd)` to `` (`${c.at}-${c.cmd}-${i}`) `` — appending the list index makes it
unique by construction (the index is already available; the list is a bounded recent-calls window, so an
index-inclusive key is stable enough for this display). One-line change in
`src/lib/components/DiagnosticsOverlay.svelte`. Verified in the running app: the exception is gone and
navigation renders normally with diagnostics enabled.
