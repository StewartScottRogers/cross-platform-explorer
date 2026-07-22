---
id: CPE-571
title: "Select by pattern — support multiple comma-separated globs"
type: Feature
status: Done
priority: Low
component: Frontend
tags: [ready]
created: 2026-07-17
closed: 2026-07-17
---

## Summary
"Select by pattern" (CPE-360) matches a single glob. Let it accept several **comma-separated** patterns
that match if ANY one matches — so `*.jpg, *.png` selects both image types at once.

## Acceptance Criteria
- [x] `matchesGlob(name, pattern)` splits on commas (trimming, ignoring blanks) and matches if any
      sub-pattern matches; a single pattern is unchanged; a wholly-blank list matches nothing.
- [x] Backward-compatible with all existing single-pattern behaviour.
- [x] `npm run check` clean; unit tests cover multi-pattern + blank handling.

## Resolution
`glob.ts`: `matchesGlob` now `split(",")` → trim → drop blanks → `some(globToRegExp(p).test(name))`.
Single patterns (no comma) behave exactly as before; blanks between commas are ignored. The
`PatternSelectDialog` benefits automatically (it calls `matchesGlob`). `glob.test.ts` +1 case
(`*.jpg, *.png`, blank handling); full suite **610 pass / 63 files**; `npm run check` 0/0. Pure change,
no i18n.
