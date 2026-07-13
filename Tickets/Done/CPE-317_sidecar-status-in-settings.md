---
id: CPE-317
title: Sidecar platform status in Settings (read-only)
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 30m
created: 2026-07-13
closed: 2026-07-13
---

## Summary

A first, read-only management surface (the observable slice of CPE-274): a "Sidecar
platform" section in the Settings dialog that shows whether the platform is active in
this build and which sidecars are registered, using the CPE-316 client. Self-contained
(fetches its own status on mount); the interactive controls (enable/disable, health,
logs) stay with CPE-274 once the supervisor runtime is wired into the app.

## Acceptance Criteria

- [ ] Settings shows platform status: Off (feature not built in) / On — no sidecars /
      On — <ids>.
- [ ] Degrades gracefully when the `sidecar-platform` feature is off (shows Off).
- [ ] `npm run check` + production build pass.

## Work Log
2026-07-13 — Filed and picked up during dayshift.
2026-07-13 — Added a read-only 'Sidecar platform' status section to SettingsDialog (fetches platformActive()/listSidecars() on mount): shows Off (not built in) / On — no sidecars / On — <ids>. Self-contained, no new App wiring. Verified: svelte-check 0/0, production build ok, vitest 260 pass. Visual verification pending the user (headless build/type-check only). Done.
