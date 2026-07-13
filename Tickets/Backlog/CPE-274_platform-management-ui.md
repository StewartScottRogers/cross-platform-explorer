---
id: CPE-274
title: Platform management UI (enable/disable, health/status)
type: Task
status: Open
priority: Medium
component: Frontend
estimate: 2-3h
created: 2026-07-13
---

## Summary

A small settings surface to see installed sidecars, their version/health/status,
and enable/disable each. The user's control panel over which Mega-Features are
active — and the place a crashed/incompatible sidecar surfaces its error.

## Acceptance Criteria

- [ ] Lists registered sidecars with name, version, contract compat, running state.
- [ ] Enable/disable toggles start/stop via the supervisor.
- [ ] Surfaces health, last error, and a link to the sidecar's log.
- [ ] Disabling a sidecar leaves the explorer and other sidecars untouched.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-265]], [[CPE-264]]. **Phase:** P5. **Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
