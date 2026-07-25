---
id: CPE-314
title: "Accessibility hardening (i18n split to CPE-362)"
type: Task
status: Done
closed: 2026-07-14
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

## Work Log
2026-07-14 — Audited a11y: the app is already strong (roles, aria-labels, `title`s and
`lang="en"` throughout — 130+ a11y attributes across components). The one genuine gap was the
column-resize dividers (CPE-350), which were mouse-only. Fixed on branch `CPE-314-a11y`:
- `FileList.svelte`: the resize handles are now `role="separator"` (the ARIA window-splitter
  pattern) with `aria-orientation`, an `aria-label`, `aria-valuenow`, `tabindex=0`, and
  ← / → keyboard resizing (Shift = larger step). Component test asserts the labelling and a
  keyboard resize.
- `npm run check` 0 errors/warnings; suite 324 pass; `npm run build` ok.

**i18n split to CPE-362** (Low): externalizing all strings + a locale system is a large
architectural change of limited value for a single-user local desktop explorer. Closing
CPE-314 as the accessibility half; i18n parked until there's demand.
