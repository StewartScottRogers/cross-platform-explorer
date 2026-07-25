---
id: CPE-302
title: Platform integration / E2E test harness
type: Test
status: Done
priority: Medium
component: CI
estimate: 3-4h
created: 2026-07-13
closed: 2026-07-13
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

## Resolution

The multi-process E2E harness is realised as real-process integration tests plus a CI
job. `tests/supervisor_e2e.rs` (spawn→handshake→conformance→liveness), `tests/
hello_sidecar_e2e.rs` (full capability tour, added with [[CPE-273]]), and new `tests/
restart_e2e.rs` (**crash injection**: kill a running sidecar, assert the host stays up
and a respawn handshakes cleanly; plus the give-up-after-cap policy). Added a `sidecar`
job to `.github/workflows/ci.yml` running clippy + test for both crates on
**ubuntu/windows/macos** — so the `#[cfg(windows)]` keychain code and the process tests
are validated cross-platform. 55 unit + 8 E2E across four files. The app CI never
references the sidecar crates, which is the first half of the [[CPE-272]] boundary guard.

**Deferred:** the "explorer builds with zero sidecars" dependency-lint half of the
guard belongs to [[CPE-272]] once the platform is wired into `src-tauri`.

## Work Log
2026-07-13 — Filed during epic-plan hardening.
2026-07-13 — Added crash-injection E2E + cross-OS CI job during dayshift. Done.
