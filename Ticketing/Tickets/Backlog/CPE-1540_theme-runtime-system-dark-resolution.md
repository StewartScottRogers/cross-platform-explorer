---
id: CPE-1540
title: "Dark theme: theme.ts resolves system prefers-color-scheme + adds explicit \"dark\" + live OS-change watch"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1493
created: 2026-08-09
---
## Context
`src/lib/theme.ts` (CPE-1535) was built with exactly one extension point in mind — its own doc comment
says so: *"CPE-1493 has exactly ONE place to extend (resolveTheme) when real dark values land: swap the
unconditional 'light' for a `matchMedia("(prefers-color-scheme: dark)")` check."* Today `ThemeSetting`
(`src/lib/types.ts:23`) is `"system" | "light"`, `resolveTheme` always returns `"light"`, and nothing
subscribes to OS theme changes. This ticket does exactly that extension, plus adds the explicit `"dark"`
override option, plus a live-update subscription so `system` tracks a running OS theme flip without a
restart.

## Scope
- `src/lib/types.ts:23` — widen `ThemeSetting` to `"system" | "light" | "dark"`.
- `src/lib/theme.ts`:
  - `ResolvedTheme` becomes `"light" | "dark"`.
  - `resolveTheme(pref)`: `"light"` → `"light"`; `"dark"` → `"dark"`; `"system"` → checks
    `window.matchMedia?.("(prefers-color-scheme: dark)").matches` and returns `"dark"` if true, else
    `"light"` (guard the `matchMedia` call — `typeof window === "undefined"` or no `matchMedia` support
    falls back to `"light"`, so this stays safe in any non-browser test context).
  - New `watchSystemTheme(onChange: () => void): () => void` — attaches a `matchMedia(...).addEventListener("change", onChange)` listener and returns an unsubscribe function. Pure, framework-free,
    same style as the rest of the module.
  - **Why `matchMedia` and not Tauri's `getCurrentWindow().theme()`/`onThemeChanged`** (the epic brief's
    first-listed option): the webview already reflects the OS light/dark signal through
    `prefers-color-scheme` on Windows/macOS/Linux without any Tauri API or new `capabilities/default.json`
    permission, and it sidesteps the Linux `onThemeChanged` flakiness the epic brief calls out (Tauri
    #9427) — one code path, no Rust surface, fully mockable in `vitest`/jsdom by stubbing
    `window.matchMedia`. Record this as the resolved design choice in the module's doc comment.
- `src/main.ts:27` (one line, right after the existing `applyTheme(loadTheme())` bootstrap call): call
  `watchSystemTheme(() => applyTheme(loadTheme()))` so a live OS flip re-resolves and re-applies — safe to
  call unconditionally since `resolveTheme` only reacts to the OS signal when the persisted pref is
  `"system"`; a `"light"`/`"dark"` pref is unaffected by the callback firing.
- `src/lib/settings.ts:185` — extend the `isTheme` validator (`v === "system" || v === "light"`) to also
  accept `"dark"`.

## How
- Test `window.matchMedia` by defining `window.matchMedia = vi.fn().mockReturnValue({ matches: bool,
  addEventListener: vi.fn(), removeEventListener: vi.fn() })` (or similar) in `theme.test.ts` — jsdom does
  not implement `matchMedia` natively, this is the standard stub pattern.
- Update `src/lib/theme.test.ts`: keep the existing `"light"`→`"light"` and add `"dark"`→`"dark"`
  (unconditional) cases; add `"system"` cases for both a `matches: true` (dark) and `matches: false`
  (light) mock, plus a no-`matchMedia`-available fallback case; add a `watchSystemTheme` test asserting
  the callback fires on a mocked `change` event and the returned unsubscribe function detaches it.
- Update `src/lib/settings.test.ts`'s theme-validator coverage to include `"dark"` as accepted and confirm
  an unrecognised value still degrades to the `"system"` default (existing corrupt-value-safety pattern).

## Verify
`npx vitest run src/lib/theme.test.ts src/lib/settings.test.ts`; `npm run check`. Fully headless — no
CSS/visual dependency; `dataset.theme` may now read `"dark"` in a live browser once the OS is dark, but
since CPE-1539's CSS block is a parallel, independent ticket, this alone still produces **no visual
change** if CPE-1539 hasn't landed yet (an unmatched `data-theme="dark"` selector is inert, same
reasoning CPE-1535 used for `data-theme="light"` before CPE-1534 landed).

## Notes
**Conflict surface:** `src/lib/theme.ts`, `src/lib/theme.test.ts`, `src/lib/types.ts` (one union-type
line), `src/lib/settings.ts` (one line in `isTheme`), `src/lib/settings.test.ts`, `src/main.ts` (one new
line after the existing bootstrap call). No `SettingsDialog.svelte` or `app.css` edits — the Settings UI
picking up the new `"dark"` value is CPE-1541. Independent of CPE-1539 — different files, no compile-time
dependency. **Dispatch order:** can run in parallel with CPE-1539; CPE-1541 dispatches after this one
(needs the widened `ThemeSetting` to typecheck a "Dark" menu option).
