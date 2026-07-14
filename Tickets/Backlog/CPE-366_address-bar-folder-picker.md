---
id: CPE-366
title: "Explorer address bar: add a Browse-folder picker (avoid typing mistakes)"
type: Feature
status: Open
priority: Medium
component: Frontend
created: 2026-07-14
---

## Summary

Standing rule: every path-picking component must offer a native modal picker in addition to
typing. Audit result — the only gap is the **explorer address bar** (NavToolbar's type-a-path
field, Ctrl+L / Alt+D): it has autocomplete (CPE-361) but no picker, so a deep path must be typed
by hand. Add a **Browse** control that opens the native folder dialog and navigates to the chosen
folder. (AI Console cwd — CPE-354 — and explorer Copy/Move-to — CPE-355 — already comply.)

## Design (frontend)
- `NavToolbar.svelte`: a folder-open button in the nav toolbar (next to Back/Forward/Up/Refresh,
  always available — more discoverable than only in edit mode) that dispatches a `browse` event.
  Optionally also a Browse button beside the path-edit input when editing.
- `App.svelte`: on `browse`, `open({ directory: true, defaultPath: currentPath })` from
  `@tauri-apps/plugin-dialog` (already imported for Copy/Move-to) → `navigate(dest)`.

## Acceptance
- A visible Browse/folder button in the address area opens the OS folder dialog; choosing a
  folder navigates there; cancel is a no-op. `npm run check` + `npm test` green.

## Notes
Filed from an app-wide path-input audit (user rule). See memory `path-inputs-need-picker`.
