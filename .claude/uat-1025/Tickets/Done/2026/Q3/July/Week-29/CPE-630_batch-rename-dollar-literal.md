---
id: CPE-630
title: "Batch rename find/replace mangled `$` in the replacement"
type: Bug
component: Frontend
priority: medium
status: Done
tags: ready
estimate: 15m
created: 2026-07-18
closed: 2026-07-18
---

## Summary
`planFindReplace` passed the replacement string to `String.prototype.replace` as a plain string, so
`$`-sequences were interpreted as replacement patterns (`$&` = the match, `$1` = a group, `$$` = `$`).
Renaming "v1" → "$100" produced "00" (the `$1` group ref, empty), "US$" lost handling, etc. The
replacement must be treated literally.

## Acceptance Criteria
- [x] The replacement is applied literally; `$`, `$&`, `$1`, `$$` in the replacement are kept verbatim.
- [x] A regression test covers `$100`, `$&`, and `US$`.
- [x] `npm run check` clean; batchRename suite green.

## Resolution
`src/lib/batchRename.ts`: use a function replacer (`from.replace(re, () => replace)`) so the replacement
is never parsed for `$` patterns. Added a CPE-630 test.

## Work Log
2026-07-18 (dayshift) — Found auditing batchRename's pure transforms; the case/number/affix plans were
correct, but find/replace leaked regex replacement semantics into user text.
