---
id: CPE-576
title: "Content search — remember the Match-case toggle"
type: Feature
status: Done
priority: Low
component: Frontend
tags: [ready]
created: 2026-07-17
closed: 2026-07-17
---

## Summary
"Search in files" resets its **Match-case** toggle every time. Persist it so your case preference sticks.

## Acceptance Criteria
- [x] The Match-case toggle restores its last value on open (localStorage).
- [x] `npm run check` clean; a component test covers restore.

## Resolution
`ContentSearchDialog` initialises `caseSensitive` from `localStorage cpe.contentSearchCase` and a reactive
`$:` persists it. Behaviour-only (no i18n). `ContentSearchDialog.test.ts` +1 (saved `"1"` → checkbox
checked on open) + a `beforeEach` localStorage clear to isolate the suite (it also persists recent
queries, CPE-558). Full suite **616 pass / 63 files**; `npm run check` 0/0.
