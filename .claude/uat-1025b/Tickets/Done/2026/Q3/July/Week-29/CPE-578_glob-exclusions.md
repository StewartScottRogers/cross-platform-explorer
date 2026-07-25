---
id: CPE-578
title: "Select by pattern — !-prefixed exclusion patterns"
type: Feature
status: Done
priority: Low
component: Frontend
tags: [ready]
created: 2026-07-17
closed: 2026-07-17
---

## Summary
Follow-on to CPE-571 (comma-separated globs). Support **exclusions**: a pattern prefixed with `!` removes
matches, so `*.js, !*.min.js` selects every `.js` except minified ones, and `!*.tmp` selects everything
except temp files.

## Acceptance Criteria
- [x] `!pattern` excludes; a name matches if it matches an include (or there are no includes) AND matches
      no exclusion.
- [x] Only-exclusion lists select everything-except; a bare `!` is ignored; blank still matches nothing.
- [x] Backward-compatible with single + comma-separated includes.
- [x] `npm run check` clean; unit tests cover exclusions.

## Resolution
`glob.ts::matchesGlob` splits the comma list into `includes` and `!`-stripped `excludes`; returns
`(includes.length === 0 || some include matches) && no exclude matches`. Single/multi include patterns
behave exactly as before. `glob.test.ts` +1 (`*.js, !*.min.js`, `!*.tmp`, bare `!`). Full suite **618 pass
/ 63 files**; `npm run check` 0/0. Pure, no i18n; `PatternSelectDialog` benefits automatically.
