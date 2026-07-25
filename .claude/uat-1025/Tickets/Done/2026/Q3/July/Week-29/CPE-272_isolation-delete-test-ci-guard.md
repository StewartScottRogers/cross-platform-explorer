---
id: CPE-272
title: Isolation "delete-test" + CI guard
type: Task
status: Done
priority: High
component: CI
estimate: 2-3h
created: 2026-07-13
closed: 2026-07-13
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

## Resolution

Wired the platform into `src-tauri` behind the Cargo feature **`sidecar-platform` (default OFF)**: `sidecar-host` is an optional dependency, and a first gated Tauri command (`sidecar_registry_ids`, feature-gated incl. its `generate_handler!` entry) loads the sidecar registry from the app's resource + config dirs. **Verified locally:** the default `cargo check`/`clippy` (no feature) — the delete-test, explorer builds with zero sidecar code — and `--features sidecar-platform` both pass, plus `npm run check`. **CI guard added** to `.github/workflows/ci.yml`: the default backend job IS the delete-test; a new step builds the app with the feature (proves the integration compiles cross-OS); and a grep step fails CI if any sidecar crate ever depends on the explorer app (the one-way rule). Documented in ADR 0001.

**Note:** this is the P4 integration seam; the actual UI pane mount is [[CPE-271]], which builds on this command.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Wired the feature-flag integration + delete-test CI guard during dayshift. Done.
