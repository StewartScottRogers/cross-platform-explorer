---
id: CPE-1493
title: "EPIC: OS light/dark detection + real dark palette (follow the system theme)"
type: Task
status: Proposed
priority: Medium
component: Frontend
tags: [epic]
created: 2026-08-08
closed:
---

> **Filed 2026-08-08 (sprint PM, theme-engine research pass).** Dormant brief — decompose on
> `/ticketing-epic activate CPE-1493`. **Depends on CPE-1492 (token foundation). Epic #2 of 5.**

## Why
The visible payoff of the theme engine: the app **follows the OS light/dark preference** and can be manually
overridden. Tauri gives light/dark detection for free (`getCurrentWindow().theme()` + the
`tauri://theme-changed` / `onThemeChanged` event, macOS app-wide) so the plumbing is cheap — the real work is
authoring a good **dark palette** across the token set.

## Scope
- Wire `theme.ts` (from CPE-1492) to `getCurrentWindow().theme()` + subscribe `onThemeChanged` → swap
  `data-theme` when the user's choice is `system`. Linux detection is best-effort (Tauri #9427 flakiness) — the
  manual override is the backstop; consider the `dark-light` crate if the Tauri event proves unreliable.
- **Author the dark Layer-1 palette** (dark color ramps) so every Layer-2 semantic token has a proper dark
  value — visual QA across all ~120 components is the bulk of the effort, not the plumbing.
- Extend Settings Appearance to `system | light | dark`.
- Verify `launcher.html` (AI Console) system colors still match in dark; verify menus/tabs/dialogs/pills all
  read correctly per their design standards; verify dialog borders stay visible ([[dialogs-need-visible-border]]).

## Verify
`npm run check`; manual/visual pass light AND dark; the gui-smoke visual leg (once CPE-1481 green) can baseline
both. No new heavy deps (Tauri event is built-in; `dark-light` only if needed).

## Notes
Frontend-heavy (palette authoring + QA); ~1 line of Rust/config if any. This is where "the app has a dark mode"
becomes true. Ship docs per CPE-579.
