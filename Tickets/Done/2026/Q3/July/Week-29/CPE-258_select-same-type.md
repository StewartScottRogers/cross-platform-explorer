---
id: CPE-258
title: Select all of this type
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 30m
created: 2026-07-13
closed: 2026-07-13
---

## Summary

Right-click a file → **Select all .ext** to select every file of the same type
in the current view — pairs with the multi-select workflow (compress, batch
rename, delete). Pure, unit-tested logic.

## Acceptance Criteria

- [x] `sameTypeIndices(entries, ext)` in filetypes.ts (dirs never match; empty
      ext = extensionless files), unit-tested.
- [x] Context menu shows **Select all .<ext>** when one file is selected.
- [x] Selecting sets the selection to all matching rows via `selectIndices`.
- [x] `npm run check` and vitest pass.

## Resolution

Added pure `sameTypeIndices` to filetypes.ts and wired a **Select all .ext** row
into the item context menu (shown for a single non-dir selection), setting the
selection to all matching visible rows via the `selectIndices` helper from
[[CPE-256]]. Frontend-only.

## Work Log
2026-07-13 — Filed, implemented, and landed during Nightshift. Verified: vitest
257 pass (3 new sameTypeIndices tests), npm run check clean. Branch
cpe-258-select-same-type.
