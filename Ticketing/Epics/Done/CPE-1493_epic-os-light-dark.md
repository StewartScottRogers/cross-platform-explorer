---
id: CPE-1493
title: "EPIC: OS light/dark detection + real dark palette (follow the system theme)"
type: Task
status: Done
priority: Medium
component: Frontend
tags: [epic]
created: 2026-08-08
closed: 2026-08-29
---

> **Filed 2026-08-08 (sprint PM, theme-engine research pass).** Activated 2026-08-09 (sprint PM, bench
> refill) — decomposed into child tickets below. **Depends on CPE-1492 (token foundation, shipped
> 2026-08-09). Epic #2 of 5.**

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

## Child tickets (activated 2026-08-09, sprint PM bench refill)
1. **CPE-1539** — Author the dark Layer-1/Layer-2 palette in `app.css`
   (`:root[data-theme="dark"]`) + a WCAG contrast guard test. Inert alone (nothing sets
   `data-theme="dark"` yet). *(independent; parallel with 1540)*
2. **CPE-1540** — `theme.ts`: `resolveTheme` honors `prefers-color-scheme` for `"system"` + adds
   explicit `"dark"` + a `watchSystemTheme` live-update subscription wired into `main.ts`. Chose
   `matchMedia` over Tauri's `onThemeChanged` (avoids the Linux flakiness the brief calls out and needs
   no new capability). *(independent; parallel with 1539)*
3. **CPE-1541** — Settings → Appearance: add the "Dark" option to `THEME_OPTIONS` + refresh the note
   copy. *(prereq: 1540 — needs the widened `ThemeSetting`)*
4. **CPE-1542** — Update `35-appearance.md` for the shipped System/Light/Dark control (no
   `sectionDocs.ts` change — the `appearance` section is already registered). *(prereq: 1541)*

Dispatch order: {1539 ∥ 1540} → 1541 → 1542. Actual dark-palette *aesthetics* (not just WCAG contrast
compliance) is genuinely subjective — CPE-1539 lands on contrast-test-green and explicitly queues an
attended visual sign-off pass once 1539+1540 are both live, rather than treating headless contrast
math as a stand-in for taste. CPE-1494/1495/1496 (native accent, window materials, theme-picker a11y)
remain the follow-up epics, sequenced after this one per the program's own dependency chain.

## Closed 2026-08-29

Closed 2026-08-29 (closeout audit) WITH ONE RESIDUAL. All 4 children Done.

Verified: a real dark palette is authored (dark primitives plus a `[data-theme="dark"]` semantic layer and hljs overrides), and the contrast guard genuinely asserts - `app.css.dark-contrast.test.ts` plus the solid-fill and accent-text sweeps. OS detection is live via `matchMedia("(prefers-color-scheme: dark)")` with a `change` subscription; matchMedia was chosen over Tauri's `onThemeChanged` **and the reason is documented at the site** (it avoids the Linux flakiness the brief names). The System/Light/Dark picker always allows a manual override.

RESIDUAL - the epic explicitly queued an **attended aesthetic sign-off** of the dark palette, and no record of that pass exists. The file itself warns not to read contrast-green as taste-approved. Headless contrast is green everywhere; only the subjective taste review is outstanding, and it needs a human, not a build.
