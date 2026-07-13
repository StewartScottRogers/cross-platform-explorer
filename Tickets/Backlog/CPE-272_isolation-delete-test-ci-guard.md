---
id: CPE-272
title: Isolation "delete-test" + CI guard
type: Task
status: Open
priority: High
component: CI
estimate: 2-3h
created: 2026-07-13
---

## Summary

Make the charter's boundary rule machine-enforced. The explorer must build, ship,
and run with **every sidecar removed**, and removing one sidecar must not affect
another. Wire this as a CI job so entanglement is caught automatically, forever.

## Acceptance Criteria

- [ ] Sidecar hosting is behind a feature flag / optional wiring; explorer compiles
      and runs with all sidecars absent.
- [ ] CI job builds the explorer with zero sidecars and asserts success + that the
      plain explorer test suite passes.
- [ ] A dependency check fails CI if any sidecar crate is imported by `app_lib` or
      vice-versa (one-way dependency enforced).
- [ ] Documented as the definitive boundary gate in the ADR [[CPE-259]].

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-271]], [[CPE-273]]. **Phase:** P4 (closes Platform MVP).
**Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
