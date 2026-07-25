---
id: CPE-808
title: Harden Condition validation in tolerant rule parsers (crash on corrupt settings)
type: bug
status: Done
priority: medium
component: Frontend
tags: ready
created: 2026-07-20
closed: 2026-07-20
epic: CPE-709
estimate: 30m
---

## Summary
Found during a nightshift self-review sweep. The tolerant rule parsers `colorRulesStore.parseRules`
(CPE-776) and `watchRules.parseRules` (CPE-793) validated a persisted rule's condition only shallowly —
colorRulesStore checked that `when.kind` was a known string; watchRules checked only `!!when`. So a
corrupted or hand-edited setting like `{kind:"ext"}` (a known kind, but no `exts`) **survived parsing**
and then threw at runtime in `matchesCondition` — `cond.exts.some(...)` on `undefined` — crashing the row
renderer / watch planner. The whole point of a tolerant parser is to drop such a landmine, not keep it.

## Acceptance Criteria
- [x] A known-kind condition with missing/mistyped fields is dropped by both `parseRules` variants.
- [x] Both parsers share **one** `Condition` validator so they can't drift again.
- [x] `npm run check` + full vitest suite green.

## Resolution
Added an exported `isValidCondition(x): x is Condition` next to the `Condition` type + `matchesCondition`
in `src/lib/colorRules.ts` — it validates the *fields* per kind (`ext` → `exts` is a string[]; `glob` →
`pattern` string; `size` → numeric `min`/`max` when present; `olderThan`/`newerThan` → numeric `days`;
`isDir` → boolean `value`; unknown kind → false). Both `colorRulesStore.ts` and `watchRules.ts` now use it
in their `isRule` guard (removing colorRulesStore's shallow local `isCondition` and watchRules' `!!o.when`
check), so a malformed condition is dropped at parse time rather than crashing later.

Tests: colorRulesStore gains a case dropping known-kind-but-malformed conditions across all six kinds;
watchRules gains a case dropping a rule whose `when` is `{kind:"ext"}` (no exts) or `{}`. Full suite
860/860 green; `npm run check` clean.

Note (not fixed here — separate concern): substring username redaction (CPE-801 `redactEvents`) can
over-match a short username inside an unrelated path segment; that is documented, inherent behavior of
substring masking, not a crash. The audit-journal `record`→`trim` sequence is also not atomic under
concurrent same-session appends (best-effort, opt-in) — left as-is.
