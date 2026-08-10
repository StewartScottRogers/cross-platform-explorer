---
id: CPE-1534
title: "Theme foundation: layer app.css into raw palette + semantic tokens (no visual change)"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1492
created: 2026-08-09
---
## Context
CPE-1492 ("Theme-token foundation") is being activated for this sprint. It's epic #1 of 5, strictly
serial before CPE-1493/1494/1495/1496 (OS light/dark, native accent, window materials, theme-picker
a11y) — none of those can start until this seam exists. Today `src/app.css` has **one** hardcoded
`:root { color-scheme: light; ... }` block (`src/app.css:2-73`, ~40 semantic vars like `--bg`,
`--surface`, `--text`, `--accent`, `--agent-1..6`) with raw hex literals inline. 114 of ~120 components
already consume colour only via `var(--...)` (the MENUS.md/TABS.md discipline), so this is a **pure
token-layering refactor**, not a component rewrite: this ticket is the foundation slice that makes the
seam exist, with **zero visual change**. It does not build the runtime that switches themes (CPE-1535)
or the Settings UI (CPE-1536) — just the CSS structure those plug into.

## Scope
- Split the single `:root` block (`src/app.css:2-73`) into two layers:
  - **Layer 1 = palette**: the raw colour ramps (hex values), named as palette tokens (e.g.
    `--palette-gray-100`, `--palette-blue-600` — pick a small, sensible naming scheme; don't over-build,
    this only needs to cover the values already in use today).
  - **Layer 2 = semantic tokens**: the existing names (`--bg`, `--surface`, `--text`, `--accent`,
    `--agent-1..6`, etc., unchanged) resolving to palette vars, defined under bare `:root` (kept as the
    light-theme safety fallback, exactly as today) **and** duplicated under `:root[data-theme="light"]`
    so a future theme runtime (CPE-1535) can select it explicitly.
- Every existing semantic var name and resolved value stays **byte-identical** in what it resolves to —
  this is a refactor, not a redesign. No component file changes.
- Do **not** touch the second, unrelated `:root { --filelist-cols: ... }` block at `src/app.css:432`
  (layout tokens, not colour — out of scope).
- No new theming framework, no new dependency — CSS custom properties only, per the epic's Notes.

## How
- Work only in `src/app.css`. Keep the `font-family`/`font-size`/`color`/`background` shorthand lines
  currently inside `:root` (`src/app.css:68-72`) in the semantic-token layer (they reference `--text`/
  `--bg`, so they follow those vars naturally).
- Add a small guard test (new `src/app.css.test.ts` or extend an existing lib test file) that reads
  `src/app.css` as text and regex-asserts: (a) every semantic token name that exists today (grep the
  current file for the list before editing, use it as the fixture) still has a declaration under
  `:root[data-theme="light"]`, and (b) no *component* file (`src/**/*.svelte`, excluding `app.css`)
  gained a new hard-coded `#rrggbb`/`#rgb` literal in files this ticket touches — this is the epic's own
  Verify line ("grep-confirm no component regressed to hard-coded hex") turned into a standing CI check
  so the seam this epic builds doesn't silently erode as the 4 follow-on theme epics land on top of it.

## Verify
`npm run check`; the new guard test (`npx vitest run`) confirming every current semantic token resolves
under `:root[data-theme="light"]` with no name dropped; visual result is pixel-identical to today since
this is a pure refactor with no runtime yet consuming `data-theme` (CPE-1535 adds that). Fully headless —
no GUI verification required to land it.

## Notes
**Conflict surface:** `src/app.css` only (the first `:root` block, lines 2-73) plus one new/extended test
file. No `src/App.svelte` edits. Independent of CPE-1535/1536/1537 — different files, no compile-time
dependency — but functionally inert alone (nothing sets `data-theme` yet until CPE-1535 lands); either
build order is fine. **Dispatch order:** can run in parallel with CPE-1535.
