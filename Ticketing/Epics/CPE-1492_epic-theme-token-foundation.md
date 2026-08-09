---
id: CPE-1492
title: "EPIC: Theme-token foundation — the load-bearing prerequisite for cross-platform theming"
type: Task
status: Proposed
priority: Medium
component: Frontend
tags: [epic]
created: 2026-08-08
closed:
---

> **Filed 2026-08-08 (sprint PM, theme-engine research pass — see research-library
> `cross-platform-theme-engine-2026-08-08`).** User directive: a cross-platform theme engine that respects
> each platform's native conventions. **Dormant brief — decompose on `/ticketing-epic activate CPE-1492`.**
> **This is epic #1 of 5 and is strictly serial before CPE-1493/1494/1495/1496.**

## Why (do this first — everything else depends on it)
CPE is light-only today: `src/app.css` has ONE hardcoded `:root { color-scheme: light; ... }` block (~40
semantic vars). But **114 of ~120 components already consume colors only via `var(--...)`** (the MENUS.md /
TABS.md discipline), so theming is a **token-layering** job, not a component rewrite. This epic builds the seam
every other theme epic slots into.

## Scope
- **Layer the tokens** (no component edits): split the single `:root` into **Layer 1 = palette** (raw color
  ramps) + **Layer 2 = semantic tokens** (the existing names — `--bg`, `--surface`, `--text`, `--accent`,
  `--agent-1..6`, etc.) resolving to palette vars under `:root[data-theme="..."]`. Keep bare `:root` defined
  (light) as the safety fallback.
- **Tiny runtime** `src/lib/theme.ts`: read persisted user choice (default `system`); for now `system` resolves
  to `light` only (no real dark values yet — just the plumbing); set `documentElement.dataset.theme`.
- **Settings**: add an **Appearance** section to `SettingsDialog.svelte` with a `system | light` stub (grows in
  CPE-1493+).
- **Do NOT** add a theming framework (Radix/Panda) — CSS custom properties + the small runtime only
  (fast/small/predictable). No new deps.

## Verify
`npm run check`; grep-confirm no component regressed to hard-coded hex; the app looks pixel-identical to today
(pure refactor). gui-smoke exercises it once CPE-1481's suite is green.

## Notes
Keep the `sidecar/ai-console/src/launcher.html` system-color contract (`Canvas`/`AccentColor`) in mind — it
tracks OS theme for free and must still match after the palette is restructured (verify, no edit expected).
~95% frontend. Ship docs per CPE-579.
