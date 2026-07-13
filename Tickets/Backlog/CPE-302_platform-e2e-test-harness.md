---
id: CPE-302
title: Platform integration / E2E test harness
type: Test
status: Open
priority: Medium
component: CI
estimate: 3-4h
created: 2026-07-13
---

## Summary

Unit tests per ticket aren't enough for a multi-process system. Build an
integration harness that boots the real host + the hello sidecar and exercises
spawn → handshake → capability calls → UI mount → crash/restart → shutdown as one
flow, runnable in CI headless on all three OSes.

## Acceptance Criteria

- [ ] Automated E2E scenario: full lifecycle with the hello sidecar, asserting
      each stage.
- [ ] Crash-injection test: kill the sidecar, assert host stays up + auto-restart.
- [ ] Runs headless in CI (Windows/macOS/Linux); flake-resistant with timeouts.
- [ ] Extended by tenant epics (AI Console adds its own scenarios).

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-273]], [[CPE-271]]. **Phase:** P4. **Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
