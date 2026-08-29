---
id: CPE-1492
title: "EPIC: Theme-token foundation — the load-bearing prerequisite for cross-platform theming"
type: Task
status: Done
priority: Medium
component: Frontend
tags: [epic]
created: 2026-08-08
closed: 2026-08-29
---

> **Filed 2026-08-08 (sprint PM, theme-engine research pass — see research-library
> `cross-platform-theme-engine-2026-08-08`).** User directive: a cross-platform theme engine that respects
> each platform's native conventions. Activated 2026-08-09 (sprint PM, bench refill) — decomposed into
> child tickets below.
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

## Child tickets (activated 2026-08-09, sprint PM bench refill)
1. **CPE-1534** — Layer `app.css` into raw palette + semantic tokens under bare `:root` (fallback) and
   `:root[data-theme="light"]`; zero visual change. *(independent; parallel with 1535)*
2. **CPE-1535** — `theme.ts` runtime + persisted `theme` setting (`system | light`, resolves to `light`
   only for now) + one bootstrap call in `main.ts`. *(independent; parallel with 1534)*
3. **CPE-1536** — Settings → Appearance section: `system | light` select wired to CPE-1535. *(prereq:
   1535)*
4. **CPE-1537** — Appearance docs page (`35-appearance.md`) + `sectionDocs.ts` registry entry per
   CPE-579. *(prereq: 1536)*

Dispatch order: {1534 ∥ 1535} → 1536 → 1537. Not decomposed further — CPE-1493 (OS light/dark, the first
epic that needs a real dark palette) is a follow-up epic once this seam has landed.

## Closed 2026-08-29

Closed 2026-08-29 (closeout audit). All 4 children Done.

Verified: `src/app.css` is layered exactly as specified - palette primitives, semantic tokens at bare `:root` as the fallback, duplicated value-identically under `[data-theme="light"]`. `theme.ts` is a small runtime (`resolveTheme`/`applyTheme`/`watchSystemTheme`) with no framework and no new dependency, and `main.ts` stamps `dataset.theme` **before mount** to avoid a flash. Settings -> Appearance ships and persists.

The "no component regressed to hard-coded hex" line is **guarded, not asserted**: `app.css.test.ts` carries a `BASELINE_TOTAL_HEX_OCCURRENCES` ratchet, so the claim is enforced on every push rather than believed.
