---
id: CPE-793
title: Pure watched-folder rule model + planner
type: feature
status: Done
priority: medium
component: Frontend
tags: ready
created: 2026-07-20
closed: 2026-07-20
epic: CPE-734
estimate: 1-2h
---

## Summary
Foundation for watched-folder automation (epic CPE-734). A pure module (`src/lib/watchRules.ts`): rules with
a trigger `Condition` (reusing CPE-774) and an ordered action pipeline, plus a planner that resolves the
actions for a landed file — so the executor (CPE-794) and editor (CPE-795, dry-run preview) are thin.

## Scope
- `Action` = move/copy (dest) | tag (tag) | rename (template, via CPE-781). `WatchRule { id, name, when:
  Condition, actions: Action[], enabled? }`.
- `planForEntry(entry, rules, now)` → `{ rule, actions }` for the **first enabled matching** rule (rename
  actions resolved via `expandTemplate`), or `null` when none match.
- CRUD (add/rename/remove/toggle/update) + tolerant `parseRules`/`serializeRules`.
- Pure + total.

## Acceptance Criteria
- [x] `planForEntry` picks the first enabled matching rule and resolves rename templates; null when none match.
- [x] CRUD immutable/correct; parse tolerant; serialize round-trips.
- [x] Pure + dependency-light; unit tests cover matching/planning/CRUD/parse; check + suite green.

## Notes
Reuses `colorRules.ts` (CPE-774) conditions + `cmdTemplate.ts` (CPE-781). Foundation for CPE-794/795. Headless.

## Resolution
Added `src/lib/watchRules.ts` (pure): `WatchRule { when: Condition, actions: Action[] }` (move/copy/tag/
rename), `planForEntry(entry, rules, now)` → the first enabled matching rule's actions with rename templates
resolved via CPE-781 (or null); CRUD + tolerant parse/serialize. Reuses CPE-774 (matchesCondition) and
CPE-781 (expandTemplate) — no duplication. 4 tests. check 0/0. Headless. Foundation for CPE-794/795.

