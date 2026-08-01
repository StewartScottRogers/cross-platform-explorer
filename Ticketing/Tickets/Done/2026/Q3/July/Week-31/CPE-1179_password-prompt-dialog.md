---
id: CPE-1179
title: "PasswordPromptDialog — shared masked-input modal (archive password owner)"
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-31
epic: CPE-705
---

## Summary
Part of the CPE-705 GUI remainder. There is no reusable text-input/password dialog in the app
(`ConfirmDialog.svelte` has no field). Build a shared **PasswordPromptDialog** component; CPE-1182 consumes it
for encrypted-archive extract/create. New files only — no App.svelte wiring here.

## Build
- New `src/lib/components/PasswordPromptDialog.svelte` (+ `PasswordPromptDialog.test.ts`): a modal with a
  single **masked** input, Cancel/OK, Enter submits, Escape cancels, focus trap, and an optional `error` prop
  (e.g. "Wrong password — try again"). Dispatches `submit(value)` / `cancel`.
- **Visible thin border** ([[dialogs-need-visible-border]]); theme-variable colours only, no hard-coded reds
  (per docs/design/MENUS.md + dialog conventions). Reuse existing dialog styling/overlay.

## Acceptance Criteria
- [ ] jsdom test: renders; typing + OK dispatches the typed value; Escape/Cancel dispatches cancel; `error`
      prop renders an error line.
- [ ] `npm run check` + `npm test` green. No new deps.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-705). Owner of the shared password dialog; consumed by CPE-1182.
- 2026-07-31 — Done. Added `src/lib/components/PasswordPromptDialog.svelte` (+ `PasswordPromptDialog.test.ts`),
  matching `ConfirmDialog`'s overlay/border/theme-variable conventions: masked `<input type="password">`,
  Cancel/OK, Enter submits, Escape cancels, auto-focus on mount, optional `error` prop rendered inline (no
  hard-coded colours, no red). `npm run check` 0 errors; `npm test` 132 files / 1495 tests green, incl. 6 new
  tests. No new deps. New files only — no App.svelte wiring (CPE-1182 will consume it).
