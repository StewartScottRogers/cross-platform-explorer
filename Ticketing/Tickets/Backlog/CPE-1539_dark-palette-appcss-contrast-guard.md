---
id: CPE-1539
title: "Dark theme: author the dark Layer-1/Layer-2 palette in app.css + a WCAG contrast guard test"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1493
created: 2026-08-09
---
## Context
CPE-1493 ("OS light/dark detection + real dark palette") is being activated for this sprint — epic #2
of 5 in the theme program, now unblocked since CPE-1492's foundation shipped. `src/app.css` today has
exactly two colour blocks: `:root { ... }` (bare fallback) and `:root[data-theme="light"] { ... }`
(`src/app.css:14-169`), both resolving the same ~40 semantic tokens (`--bg`, `--surface`, `--text`,
`--accent`, `--agent-1..6`, etc.) to Layer-1 palette primitives (`--pal-*`). This ticket adds the third
block — `:root[data-theme="dark"]` — with real, authored dark values. It is **inert on landing**: nothing
sets `data-theme="dark"` yet (that's CPE-1540), so there is zero visual change to the shipping app.

## Scope
- Add a new Layer-1 block of dark palette primitives (e.g. `--pal-dark-gray-900: #202020`, following the
  naming convention of the existing `--pal-*` primitives) covering every colour family the light palette
  uses: neutrals/backgrounds, borders, text, accent blue, danger red, success green, and the six
  colour-blind-safe agent colours + the user/unknown agent colours.
- Add `:root[data-theme="dark"] { ... }` resolving **every** semantic token that exists in
  `:root[data-theme="light"]` today (same name list — `--bg`, `--surface`, `--surface-alt`, `--hover`,
  `--active`, `--border`, `--border-strong`, `--dialog-border`, `--danger`, `--success`, `--danger-hover`,
  `--text`, `--text-dim`, `--text-faint`, `--accent`, `--accent-hover`, `--selection`,
  `--selection-hover`, `--radius`, `--radius-lg`, `--row-h`, `--agent-1..6`, `--agent-user`,
  `--agent-unknown`, plus the `font-family`/`font-size`/`color`/`background` shorthand lines) to the new
  dark palette vars. Set `color-scheme: dark` in this block (not `light`).
- Starting values to author from (Fluent/Win11-dark-inspired; treat as a starting point — the binding
  acceptance gate is the contrast guard test below, not these exact hexes):
  window bg `#202020`, surface `#2b2b2b`, surface-alt `#262626`, hover `#333333`, active `#3d3d3d`,
  border `#3f3f3f`, border-strong `#5c5c5c`, dialog-border `#7a7a7a` (needs to read as a crisp edge over
  a dimmed backdrop on a dark bg — re-derive per the same reasoning as the existing `--dialog-border`
  comment, not just an inverted light value), text `#f3f3f3`, text-dim `#c5c5c5`, text-faint `#9c9c9c`,
  accent `#3ea6ff` / accent-hover `#60cdff` (lighter than the light-theme blue — a saturated
  `#0067c0`-style blue under-contrasts on a dark surface), selection `#0f3550` / selection-hover
  `#123f5e`, danger `#ff6659` / danger-hover `#ff8a80`, success `#6bcf7f`. Re-check the six Okabe-Ito
  agent colours + `--pal-purple-500` (agent-user) against the new dark `--bg`/`--surface` — lighten any
  that fall under the 3:1 UI-contrast bar; they don't have to match the light values.
- Do **not** touch the bare `:root` block or `:root[data-theme="light"]` block — additive only. Do not
  touch the unrelated `--filelist-cols` block (`src/app.css` around line 432).

## How
- New test file `src/app.css.dark-contrast.test.ts` (pure text + math, no jsdom/browser needed — mirrors
  how CPE-1534's guard test regex-parses `app.css` as text):
  1. Read `src/app.css`, extract every `--pal-dark-*: #rrggbb` primitive declaration into a name→hex map.
  2. Extract the `:root[data-theme="dark"]` block; for each semantic token, resolve its `var(--pal-...)`
     reference through the map to a concrete hex.
  3. Implement WCAG relative-luminance + contrast-ratio math (small local helper, ~15 lines, no new dep —
     `(L1+0.05)/(L2+0.05)` on sRGB relative luminance).
  4. Assert: `--text` vs `--bg` and vs `--surface` ≥ 4.5:1 (WCAG AA normal text); `--text-dim` vs `--bg`
     ≥ 3:1 (secondary/large text); `--accent`, `--border-strong`, `--dialog-border` vs `--bg`/`--surface`
     ≥ 3:1 (WCAG 1.4.11 non-text UI contrast); `--danger` vs `--bg`/`--surface` ≥ 4.5:1 (error text).
  5. Assert every semantic token name present under `:root[data-theme="light"]` is also present under
     `:root[data-theme="dark"]` — no dropped token (same shape-check pattern as CPE-1534).
- If a starting hex above fails its threshold, adjust the primitive (not the semantic mapping) until the
  test passes — that's the authoring loop this ticket is doing.

## Verify
`npx vitest run src/app.css.dark-contrast.test.ts` (new contrast + completeness assertions, all passing);
`npm run check`. Fully headless — no GUI needed to land it, since nothing consumes `data-theme="dark"`
yet.

**Async visual sign-off queued (not headless):** the actual *aesthetic* quality of the dark palette —
does it look good, not just pass contrast math — is genuinely subjective and cannot be judged headlessly.
Once CPE-1540/1541 wire it live, queue an attended visual pass for the user (light vs dark side-by-side,
a few representative surfaces: file list, a dialog, a menu, the tab strip) rather than treating contrast
compliance alone as "done" aesthetically. Land this ticket on contrast-test-green; flag the aesthetic
pass as outstanding in the Work Log.

## Notes
**Conflict surface:** `src/app.css` only (new `:root[data-theme="dark"]` block, additive after the
existing `:root[data-theme="light"]` block) plus one new test file. No `src/App.svelte`,
`src/lib/theme.ts`, or `SettingsDialog.svelte` edits. Independent of CPE-1540 — different files, no
compile-time dependency — but functionally inert alone until CPE-1540 makes `data-theme="dark"`
reachable. **Dispatch order:** can run in parallel with CPE-1540.
