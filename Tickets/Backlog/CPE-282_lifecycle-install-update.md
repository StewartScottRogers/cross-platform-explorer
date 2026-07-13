---
id: CPE-282
title: "Lifecycle: install / update (Rust-orchestrated package managers)"
type: Feature
status: Open
priority: High
component: Backend
estimate: 4h+
created: 2026-07-13
---

## Summary

Rust reimplementation of the `*--install.cmd` behavior: install/update an agent CLI
and its prerequisites via the right package manager per OS (winget/npm on Windows,
brew/npm/pipx on macOS/Linux), refresh PATH for the running session, and report
progress. All orchestration in Rust — the package managers are the mechanism, not
shell scripts.

## Acceptance Criteria

- [ ] `install(agent)` / `update(agent)` follow the manifest's per-OS recipe;
      ensure prerequisites (e.g. Node) first.
- [ ] Streams progress/output to the console UI; clear success/failure result.
- [ ] Post-install PATH refresh so the tool is usable without a restart.
- [ ] Idempotent (re-run updates); never silently partially-installs.
- [ ] Tested against a manifest using a trivial installable package.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-278]], [[CPE-281]]. **Phase:** C3. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
