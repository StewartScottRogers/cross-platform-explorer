---
id: CPE-003
title: Add a type-check + first test gate to CI
type: Test
status: Done
priority: Medium
component: CI
estimate: 1-2h
created: 2026-07-10
closed: 2026-07-10
---

## Summary

The repo had no automated checks on push. Add a lightweight CI job that runs `npm run check`
(svelte-check + tsc) and a minimal unit test, so regressions in the frontend are caught before a
release build.

## Acceptance Criteria

- [x] A `ci.yml` workflow runs on push and pull_request to main
- [x] It runs `npm ci` then `npm run check` and fails on type errors
- [x] A test runner (Vitest) is added with at least one real test
- [x] `npm test` script wired in package.json and run in CI
- [x] Badge added to README pointing at CI status

## Resolution

Added `.github/workflows/ci.yml` — runs on push and PR to `main`, on ubuntu-latest: `npm ci`,
`npm run check`, then `npm test`. Fast (Node only); the full cross-platform Tauri build stays in
`release.yml` on version tags.

To make the logic testable, extracted the two pure helpers out of `src/App.svelte` into a new
module `src/lib/format.ts`:

- `formatSize(bytes)` — human-readable byte formatting
- `friendlyError(raw)` — maps raw backend errors to user-facing messages (added in CPE-007)

`App.svelte` now imports both rather than defining them inline, so the component keeps only UI
concerns and the logic is unit-testable.

Added Vitest (`vitest` devDependency, `test` / `test:watch` scripts) and `src/lib/format.test.ts`
with **10 tests** covering: zero bytes, byte formatting, the exact 1024 unit-rollover boundary,
larger units, clamping at the largest known unit, all three `friendlyError` branches, the generic
fallback, and a regression test asserting `friendlyError` never leaks raw path text.

Verified locally: `npm run check` -> 0 errors, 0 warnings; `npm test` -> 10 passed.

Files changed: `.github/workflows/ci.yml` (new), `src/lib/format.ts` (new), `src/lib/format.test.ts`
(new), `src/App.svelte`, `package.json`, `README.md`.

## Work Log

2026-07-10 — Picked up. Estimate: 1-2h. Plan: extract pure helpers to a testable module, add Vitest, add ci.yml, badge.
2026-07-10 — Extracted formatSize + friendlyError from App.svelte into src/lib/format.ts; App.svelte now imports them.
2026-07-10 — Added Vitest and src/lib/format.test.ts with 10 tests, including the 1024 rollover boundary and a "never leak raw path" regression test.
2026-07-10 — Added .github/workflows/ci.yml (npm ci -> npm run check -> npm test) on push/PR to main.
2026-07-10 — Ran locally: svelte-check 0 errors/0 warnings; vitest 10/10 passed. Added CI badge to README. Closing as Done.

## Notes

The extraction was a prerequisite: `formatSize` was previously trapped inside the Svelte component
and not importable by a test. Rust-side tests are still absent — a future ticket could add
`#[cfg(test)]` coverage for `list_dir` / `parent_dir`.
