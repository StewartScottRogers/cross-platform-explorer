---
id: CPE-003
title: Add a type-check + first test gate to CI
type: Test
status: Open
priority: Medium
component: CI
estimate: 1-2h
created: 2026-07-10
closed:
---

## Summary

The repo has no automated checks on push. Add a lightweight CI job that runs `npm run check`
(svelte-check + tsc) and a minimal unit test, so regressions in the frontend are caught before a
release build. Rust is already compiled by the release workflow.

## Acceptance Criteria

- [ ] A `ci.yml` workflow runs on push and pull_request to main
- [ ] It runs `npm ci` then `npm run check` and fails on type errors
- [ ] A test runner (e.g. Vitest) is added with at least one real test (e.g. the `formatSize` helper)
- [ ] `npm test` script wired in package.json and run in CI
- [ ] Badge or note added to README pointing at CI status

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

*(Agent appends dated entries here throughout — do not fill in)*

## Notes

`formatSize` in src/App.svelte is pure and a good first unit-test target.
