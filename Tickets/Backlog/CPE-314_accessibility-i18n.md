---
id: CPE-314
title: Accessibility & i18n for console & panels
type: Task
status: Open
priority: Low
component: Frontend
estimate: 2-3h
created: 2026-07-13
---

## Summary

A quality bar the sidecar UIs must meet: keyboard operability, screen-reader labels,
sufficient contrast in light/dark, and translatable strings — so the platform's UIs
(management panel, console, launcher) match the explorer's standards and don't
become an accessibility regression.

## Acceptance Criteria

- [ ] Management UI, launcher, and console are fully keyboard-navigable with ARIA
      roles/labels; terminal has an accessible mode.
- [ ] Light/dark contrast meets the app's standard; respects theme.
- [ ] User-facing strings are externalised for translation.
- [ ] Included in the definition-of-done checklist for tenant UIs.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-271]], [[CPE-289]]. **Phase:** P5 / C6 (cross-cutting).
**Epic:** [[CPE-260]] & [[CPE-261]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
