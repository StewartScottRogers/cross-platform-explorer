---
id: CPE-581
title: "Cleanup — consolidate duplicated localStorage persistence into a shared helper"
type: Task
status: Done
priority: Low
component: Frontend
tags: [ready]
created: 2026-07-17
closed: 2026-07-17
---

## Summary
`/simplify` finding (reuse): the session's UI-state persistence work (CPE-551/556/573/575/576) duplicated
the same `try { localStorage… } catch {}` pattern as local helpers in five components. Consolidate into a
single shared helper.

## Resolution
Added `src/lib/persist.ts` — `lsGet(key)`, `lsSet(key, value)`, `lsBool(key, fallback)` (all never-throw,
storage-unavailable-safe). Migrated the per-component helpers/inline try-catches in `BoardView`,
`HomeView`, `WorkbenchView`, `ContentSearchDialog`, and `PreviewPane` to import them — removing ~10
duplicated try/catch blocks with no behaviour change. Added `persist.test.ts` (2 tests). Full suite **620
pass / 64 files**; `npm run check` 0/0.

**Skipped (noted):** `globToRegExp` is duplicated in `glob.ts` and `search.ts`, but the two differ in
case-handling (glob uses the `i` flag; search lowercases inputs), so consolidating would reconcile
matching behaviour — out of scope for a pure cleanup; left for a dedicated pass.
