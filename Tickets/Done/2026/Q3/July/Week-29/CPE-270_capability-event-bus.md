---
id: CPE-270
title: "Capability: event / notification bus"
type: Task
status: Done
priority: Medium
component: Backend
estimate: 1-2h
created: 2026-07-13
closed: 2026-07-13
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

## Resolution

Sidecar→host: `providers::events::EventRouter` forwards `Event::Notify/Progress/Status`
to a host `EventSink`, but only when the sidecar holds `Capability::Events` (ungranted
events are dropped, returns false). Host→sidecar: added `HostSignal`
(Activated/Deactivated/ThemeChanged/WillQuit) + `Message::Signal` to the contract as an
**additive change**, bumping `CONTRACT_VERSION` to **1.1** (demonstrating the semver
policy from [[CPE-263]]); `signal_envelope()` encodes one to send. All delivery is
host-mediated — no sidecar-to-sidecar path. 3 host tests (kind routing, drop-when-
ungranted, signal round-trip) + a contract round-trip test. Contract 7 + host 39 unit +
3 E2E + clippy green.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Implemented during dayshift; additive contract 1.0→1.1 bump for host
signals. Done.
