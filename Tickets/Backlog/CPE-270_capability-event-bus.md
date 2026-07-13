---
id: CPE-270
title: "Capability: event / notification bus"
type: Task
status: Open
priority: Medium
component: Backend
estimate: 1-2h
created: 2026-07-13
---

## Summary

A brokered channel for a sidecar to emit notifications/status to the host (toasts,
progress, badges) and to receive host lifecycle signals (focus, theme, shutdown).
Host-mediated only — never a direct sidecar-to-sidecar channel — preserving
isolation.

## Acceptance Criteria

- [ ] Sidecar → host: notify(level, message), progress(id, pct), status(state).
- [ ] Host → sidecar: lifecycle signals (activated, theme-changed, will-quit).
- [ ] No sidecar-to-sidecar delivery; all routing goes through the host broker.
- [ ] Tests: emit + receive via the hello sidecar.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-266]]. **Phase:** P3. **Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
