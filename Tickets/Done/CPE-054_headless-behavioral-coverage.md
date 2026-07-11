---
id: CPE-054
title: Add headless behavioral coverage for shipped features (closes most of CPE-053)
type: Test
status: Done
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

CPE-053 queued live-GUI verification of the features shipped in the supervised session. During the
Nightshift run it emerged that (a) no Rust toolchain is installed locally, so backend work can't be
built/tested here, and (b) this environment has no screenshot / click-automation tool, so literal
pixels can't be observed. HOWEVER the repo already drives real components in jsdom via
`@testing-library/svelte` (see `App.test.ts`, `FileList.test.ts`). That means feature *behavior*
(including the App wiring) CAN be verified headlessly — everything except literal visual appearance.

This ticket adds that behavioral coverage, converting most of CPE-053 from "needs a human" into
automated tests.

## Acceptance Criteria

- [ ] CPE-052 wildcard search: `App` integration test types `*.md` and asserts only matching names remain
- [ ] CPE-050 new-folder auto-number: `App` test with an existing "New folder" asserts `create_dir` is invoked with "New folder (2)"
- [ ] CPE-051 name validation: `App` test drives an inline rename to an illegal name and asserts the error notice shows and `rename_entry` is NOT invoked
- [ ] CPE-047 executable icon: `FileList` test asserts a `.exe` row renders the executable glyph (distinct stroke) and Type "Application"
- [ ] `npm run check` clean; full suite green
- [ ] CPE-053 updated to reflect what is now automated vs. the residual human-only visual checks

## Resolution

Added `src/App.features.test.ts` (3 integration tests driving the real App with a mocked backend):
wildcard search filters to `*.md`; New folder invokes `create_dir` with "New folder (2)" when a
"New folder" exists; renaming to `bad/name.md` shows the validation notice and never calls
`rename_entry`. Added a `FileList` test asserting a `.exe` renders the executable glyph (distinct
`#5b3fd6` stroke) and Type "Application". Suite 121 → 132 passing; `npm run check` 0 errors; `vite
build` clean. Updated CPE-053: behavioral half discharged, residual narrowed to a visual/aesthetic
smoke-check (Low). Committed on branch, merged to `main`, pushed.

## Work Log

2026-07-11 — Nightshift: cargo absent (no backend work) and no pixel/observe capability; pivoted to headless behavioral coverage via testing-library, which genuinely verifies the shipped features' wiring. Studied App.test.ts / FileList.test.ts harnesses and the search/rename/new-folder dispatch paths.

## Notes

UNC breadcrumb (CPE-046) is left to its unit tests: crumb rendering is the same code path as the
drive-letter case which the suite already exercises, and `splitPath` UNC handling is thoroughly unit
tested. Residual human-only check after this ticket: the *appearance* of the executable glyph.
Related: [[CPE-053]] [[CPE-047]] [[CPE-050]] [[CPE-051]] [[CPE-052]].
