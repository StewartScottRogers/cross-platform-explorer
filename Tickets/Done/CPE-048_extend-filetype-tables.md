---
id: CPE-048
title: Extend file-type tables with common missing extensions
type: Task
status: Done
priority: Low
component: Frontend
estimate: 30m
created: 2026-07-11
closed: 2026-07-11
---

## Summary

`src/lib/filetypes.ts` maps file extensions to visual categories and friendly type names, but several
common extensions are missing and fall back to "unknown" icon / "XYZ File" type. Add the obvious gaps,
mapping only to categories that already exist (no new `FileCategory`, no `Icon.svelte` change), so this
is pure, fully unit-testable logic.

## Acceptance Criteria

- [ ] Add common missing extensions to both `CATEGORY_BY_EXT` and `TYPE_NAME_BY_EXT`:
      images `heic`, `avif`, `jfif`; audio `aac`, `opus`; video `wmv`, `flv`, `m4v`; archives `xz`,
      `bz2`, `zst`, `tgz`; code `mjs`, `cjs`
- [ ] Each new extension maps to an existing category (no new category introduced)
- [ ] Unit tests in `filetypes.test.ts` assert category + type name for representatives
- [ ] `npm run check` clean; full suite green

## Resolution

Added 15 common extensions to `CATEGORY_BY_EXT` and `TYPE_NAME_BY_EXT`, all mapped to pre-existing
categories (image/audio/video/archive/code) — no new `FileCategory`, so `Icon.svelte` is untouched.
Added `categoryOf`/`typeName` tests for representatives. `npm run check` clean; full suite 93 passed.
Committed on branch `cpe-048-filetype-tables`.

## Work Log

2026-07-11 — Filed during Nightshift loop 2. Scope deliberately limited to existing categories to keep it GUI-free and verifiable via unit tests while the user is present.
2026-07-11 — Implemented; added tests. `npm run check` = 0 errors; `npm test` = 93 passed. Committed on branch `cpe-048-filetype-tables`. No GUI step required (pure classification logic, no visual change).

## Notes

Related: [[CPE-047]] (executable icon category) and the completeness test suggested there.
