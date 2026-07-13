---
id: CPE-281
title: "Lifecycle: detect / is-installed (per-OS)"
type: Task
status: Open
priority: High
component: Backend
estimate: 2-3h
created: 2026-07-13
---

## Summary

Rust implementation of each agent's `is-installed` check (ported from the
`*--is-installed.cmd` pattern): resolve the CLI on PATH, report installed/version
and configured models, per-OS. No shell scripts — Rust runs the probe and parses
the result.

## Acceptance Criteria

- [ ] `detect(agent)` returns installed?, resolved path, version, per the manifest's
      detect recipe.
- [ ] Cross-platform PATH/where/which resolution in Rust.
- [ ] Surfaces status to the launcher UI ([[CPE-289]]).
- [ ] Tests with a stub agent manifest.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-278]]. **Phase:** C2. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
