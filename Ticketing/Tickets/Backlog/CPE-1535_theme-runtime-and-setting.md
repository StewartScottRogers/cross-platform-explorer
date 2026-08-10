---
id: CPE-1535
title: "Theme foundation: theme.ts runtime + persisted setting (resolves to light only, for now)"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1492
created: 2026-08-09
---
## Context
CPE-1492's second slice: the tiny runtime that reads a persisted theme choice and stamps
`documentElement.dataset.theme` so the CSS layer CPE-1534 builds has something to select on. Per the
epic brief, for **now** `system` resolves to `light` only — there is no real dark palette yet (that's
CPE-1493). This ticket is plumbing only: a settings field + a runtime module + one bootstrap call. No
Settings UI (that's CPE-1536).

## Scope
- A new `theme: "system" | "light"` setting in `src/lib/settings.ts`, following the file's existing
  pattern exactly — see `KEYS.density`/`isDensity`/`loadDensity`/`saveDensity` (`src/lib/settings.ts:82,
  183, 235-236`, landed in CPE-1526) for the shape to copy: add `KEYS.theme = "cpe.theme"`, an
  `isTheme` validator, `loadTheme()` (default `"system"`), `saveTheme(v)`.
- A new `src/lib/theme.ts`: `resolveTheme(pref: "system" | "light"): "light"` (today always `"light"` —
  the function exists so CPE-1493 has exactly one place to extend when real dark values land) and
  `applyTheme(pref)` that sets `document.documentElement.dataset.theme = resolveTheme(pref)`.
- Wire one call into `src/main.ts`'s `bootstrap()` (after `initSettings()` resolves, before the app
  mounts) — `applyTheme(settings.loadTheme())` — so the attribute is set before first paint, avoiding a
  flash. This is the only `src/main.ts` touch: one import + one call, no restructuring.
- No new dependency (matches `settings: no new deps` convention already in this codebase).

## How
- Copy the `settings.ts` idiom used by `loadDensity`/`saveDensity` verbatim in shape/doc-comment style.
- `resetSettings()` already resets the whole `state` object, so the new key needs no special-case
  handling there (same note as CPE-1526).
- Keep `theme.ts` pure/synchronous and framework-free so it's trivially unit-testable (mock
  `document.documentElement.dataset` and `localStorage`/the settings module in tests, no real DOM theme
  effect needed since CPE-1534 owns the CSS side).
- Delete-test: an absent/corrupt stored value degrades to `"system"` → resolves to `"light"` — never
  crashes, never changes anything visible (matches today's only behaviour, since CPE-1534's `:root`
  fallback is already light).

## Verify
`npm run check`. New `src/lib/theme.test.ts`: `resolveTheme("system")` and `resolveTheme("light")` both
return `"light"`; `applyTheme` sets `dataset.theme` to `"light"`. Extend `src/lib/settings.test.ts` with
the same three cases used for `density` (default, round-trip, corrupt-value fallback) applied to
`loadTheme`/`saveTheme`. Fully headless — jsdom, no GUI verification needed (no visible effect yet).

## Notes
**Conflict surface:** `src/lib/settings.ts` (additive — new `KEYS` entry + validator + two functions,
same pattern as a dozen existing entries), new `src/lib/theme.ts` + `src/lib/theme.test.ts`, and one
import + one call added to `src/main.ts`'s `bootstrap()` function. No `src/App.svelte` edits.
Independent of CPE-1534 (different files). **Prerequisite for CPE-1536** (the Settings UI needs
`loadTheme`/`saveTheme`/`applyTheme` to exist). **Dispatch order:** can run in parallel with CPE-1534;
must land before CPE-1536.
