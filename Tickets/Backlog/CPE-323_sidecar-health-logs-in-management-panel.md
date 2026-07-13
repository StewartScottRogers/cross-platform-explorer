---
id: CPE-323
title: "Surface sidecar health, last error & logs in the management panel"
type: Feature
status: Open
priority: Low
component: Multiple
created: 2026-07-13
---

## Summary

The management panel (CPE-274) shows each sidecar's running/stopped state, but not its
**last error** or a **link to its logs**. Wire the host-side observability
(`sidecar-host::observability` — `LogCapture` / `build_diagnostics`, CPE-298/299) to the
live supervisor connection and expose it so a crashed/incompatible sidecar surfaces an
actionable error and recent log lines in the panel.

## Acceptance Criteria

- [ ] A `sidecar_diagnostics(id)` command returns last error + recent redacted log lines.
- [ ] The management panel shows a health/last-error line and a "view logs" affordance.
- [ ] Secrets never appear (redaction verified).

## Notes
Split out of CPE-274 (management UI): the list/version/compat/running/enable-disable/
capability-revoke surface shipped there; this is the diagnostics half, which needs the
`LogCapture` attached to the running `ProcessConnection`. **Depends on:** [[CPE-298]],
[[CPE-299]], [[CPE-274]].
