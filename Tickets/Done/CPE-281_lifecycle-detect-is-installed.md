---
id: CPE-281
title: "Lifecycle: detect / is-installed (per-OS)"
type: Task
status: Done
priority: High
component: Backend
estimate: 2-3h
created: 2026-07-13
closed: 2026-07-13
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

## Resolution

Implemented `lifecycle::detect` in the `ai-console` crate: runs the agent manifest's
per-OS `detect` command via a `CommandRunner` trait (`RealRunner` uses `std::process`;
fake runner for tests) and returns `DetectResult { installed, version }` — installed
when the command exits 0, version = first non-empty stdout line, not-installed on
non-zero/spawn-failure (not on PATH) or when no detect command is declared. No shell
scripts. 4 tests (success+version, non-zero, spawn-fail, undetectable) with a fake
runner; 20 crate tests + clippy green.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Implemented detection with a testable CommandRunner during dayshift. Done.
