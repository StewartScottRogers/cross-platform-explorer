---
id: CPE-1174
title: "Exclude gui-smoke/ from the root vitest glob (fix red main from node:test files)"
type: bug
component: Testing
priority: high
status: Done
tags: ready
created: 2026-07-31
epic: CPE-1148
---

## Summary
**Base-red hotfix.** After CPE-1170 (#486) added `gui-smoke/lib/compare.ts` + `compare.test.ts` and CPE-1172
(#488) added `compare.filters.test.ts`, the root `npm test` (`vitest run`) began **failing**: `vite.config.ts`'s
vitest `exclude` list did not cover `gui-smoke/`, and there is no narrowing `include`, so vitest's default glob
collected those files. They are **`node:test`** suites (run by `gui-smoke`'s own `tsx --test` via
`npm run test:unit`), not vitest suites, so vitest reported **"No test suite found"** for each and the root run
exited non-zero — reddening `main`'s CI job on the #486 commit onward.

The per-ticket gauntlets for CPE-1170/1172 ran `gui-smoke`'s own `npm run test:unit` (green) but not the root
`vitest run`, so the cross-runner collision escaped review. Detected when the CPE-1173 worker ran root
`npm test` and saw the two phantom-suite failures (reproduced on `main` with its branch stashed).

## Fix
- `vite.config.ts`: add `"**/gui-smoke/**"` to the vitest `test.exclude` array (alongside `node_modules`,
  `dist`, `.claude`, `target`). `gui-smoke/` is a separate sub-project with its own `node:test` runner; the
  root vitest must not collect it.

## Acceptance Criteria
- [x] `npm test` (root `vitest run`) green — no "No test suite found" failures. Verified: **130 files / 1482
      tests passed**.
- [x] `gui-smoke`'s own `npm run test:unit` unaffected (separate runner, still runs its node:test suites).
- [x] main CI (the `CI` workflow) goes green again on the fix commit.

## Work Log
- 2026-07-31 — Foreman hotfix during workshift. Verified root vitest green locally before merge. Ledger:
  CPE-1170 back-annotated `post_merge_defect: ci-red` (escaped defect signal). Gauntlet lesson: a
  gui-smoke-touching change must also run the ROOT `npm test`, not only `gui-smoke`'s `test:unit`.
