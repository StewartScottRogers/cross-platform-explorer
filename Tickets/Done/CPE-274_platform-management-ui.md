---
id: CPE-274
title: Platform management UI (enable/disable, health/status)
type: Task
status: Done
closed: 2026-07-13
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
2026-07-13 — **Implemented.** Backend: `sidecar-host::enablement::EnablementStore`
(disabled set, persisted; enabled-by-default; per-sidecar independent — unit-tested) +
feature-gated commands `sidecar_details` (id/name/version/contract-compat/running/enabled/
requested+granted caps), `sidecar_stop`, `sidecar_set_enabled` (disabling also stops).
`sidecar_start_ai_console` now refuses to start a disabled sidecar. Frontend:
`SidecarManager.svelte` — rows with running dot, version, contract-compat badge,
enable/disable toggle, granted-capability chips with per-cap revoke (completing CPE-296's
revoke surface), and Open/Stop for the console; embedded in Settings, hidden entirely when
the platform isn't built in. Client helpers in `sidecar.ts`.

Acceptance: list w/ name/version/compat/running ✅, enable/disable start-stop ✅,
disabling leaves others untouched ✅ (independent, tested). **Health/last-error/log link
is split to CPE-323** (needs `LogCapture` wired to the live connection). Verified: host 78
tests, feature clippy clean, default build holds delete-test, `npm run check` 0/0, 264
frontend tests. Visual verification of the panel pending a human.
