---
id: CPE-951
title: User macro library (CRUD + persist over action-macros)
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-739
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
Second headless slice of scriptable actions (CPE-739), building on `action_macro` (CPE-938).
`cpe_server::macro_library`:
- `MacroLibrary` — an ordered, JSON round-tripping store of named `ActionMacro`s with `add`/`update`
  (validate-on-save; unique case-insensitive names; rename-without-collision), `remove`, `reorder`,
  `get`/`names`.
- Also added `Eq` + `Deserialize` to `ActionMacro`/`MacroStep` so the library persists.

The layer the macro editor + toolbar/menu binding UI sits on.

## Acceptance Criteria
- [x] add validates + rejects dup names; update renames without colliding; remove/reorder; JSON round-trip.
- [x] 4 new tests; action_macro's 14 still pass; clippy clean.

## Work Log
- 2026-07-23 (dayshift) — Second CPE-739 slice: the persisted named-macro library.
