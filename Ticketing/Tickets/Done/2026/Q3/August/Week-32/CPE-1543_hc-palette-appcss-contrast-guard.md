---
id: CPE-1543
title: "High contrast: author hc-light/hc-dark Layer-1 palettes in app.css + a stricter WCAG contrast guard"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1496
created: 2026-08-09
---
## Context
CPE-1496 ("theme picker + built-in themes + high-contrast / a11y") is being activated for this sprint —
epic #5 of 5 in the theme program, unblocked now that CPE-1492 (token foundation) and CPE-1493 (real
light/dark palettes, `System`/`Light`/`Dark` picker) have both shipped. `src/app.css` today has three
colour blocks: bare `:root` (fallback), `:root[data-theme="light"]`, and `:root[data-theme="dark"]`
(the last landed by CPE-1539/CPE-1541, `src/app.css:1-268`). This ticket adds two more —
`:root[data-theme="hc-light"]` and `:root[data-theme="hc-dark"]` — authored to a **stricter** contrast
bar than the normal palettes, for OS/user-requested high-contrast mode. It is **inert on landing**:
nothing sets `data-theme` to either new value yet (that's CPE-1545/CPE-1546), so there is zero visual
change to the shipping app.

## Scope
- Add two new Layer-1 blocks of high-contrast palette primitives (e.g. `--pal-hc-light-gray-900: ...`,
  `--pal-hc-dark-gray-900: ...`, following the existing `--pal-*`/`--pal-dark-*` naming convention)
  covering every colour family the light/dark palettes use: neutrals/backgrounds, borders, text, accent
  blue, danger red, success green, and the six colour-blind-safe agent colours + user/unknown agent
  colours. High contrast typically means near-black-on-white / near-white-on-black with saturated,
  unambiguous accents — start from the existing light/dark primitives and push luminance + saturation
  toward the extremes until the guard test (below) passes.
- Add `:root[data-theme="hc-light"] { ... }` and `:root[data-theme="hc-dark"] { ... }`, each resolving
  **every** semantic token that exists in `:root[data-theme="light"]`/`:root[data-theme="dark"]` today
  (same name list — see CPE-1539 for the full enumeration: `--bg`, `--surface`, `--surface-alt`,
  `--hover`, `--active`, `--border`, `--border-strong`, `--dialog-border`, `--danger`, `--success`,
  `--danger-hover`, `--text`, `--text-dim`, `--text-faint`, `--accent`, `--accent-hover`, `--selection`,
  `--selection-hover`, `--radius`, `--radius-lg`, `--row-h`, `--agent-1..6`, `--agent-user`,
  `--agent-unknown`, plus the `font-family`/`font-size`/`color`/`background` shorthand lines). Set
  `color-scheme: light` in the `hc-light` block and `color-scheme: dark` in the `hc-dark` block.
- Append both new blocks directly after the existing `:root[data-theme="dark"]` block closes
  (`src/app.css:268`), before the `* { box-sizing: ... }` reset rule at `src/app.css:270`. Do **not**
  touch the bare `:root`, `:root[data-theme="light"]`, or `:root[data-theme="dark"]` blocks — additive
  only. Do not touch the unrelated `--filelist-cols` block (`src/app.css` around line 628).

## How
- New test file `src/app.css.hc-contrast.test.ts` (pure text + math, no jsdom/browser needed — mirrors
  `src/app.css.dark-contrast.test.ts`'s regex-parse-app.css-as-text approach, reusing/duplicating its
  small WCAG relative-luminance + contrast-ratio helper):
  1. Read `src/app.css`, extract every `--pal-hc-light-*: #rrggbb` and `--pal-hc-dark-*: #rrggbb`
     primitive declaration into two name→hex maps.
  2. Extract the `:root[data-theme="hc-light"]` and `:root[data-theme="hc-dark"]` blocks; for each
     semantic token, resolve its `var(--pal-...)` reference through the matching map to a concrete hex.
  3. Assert a **stricter** bar than the normal-mode guard (WCAG AAA-inspired, reflecting what "high
     contrast" is for): `--text` vs `--bg` and vs `--surface` ≥ 7:1; `--text-dim` vs `--bg` ≥ 4.5:1;
     `--accent`, `--border-strong`, `--dialog-border` vs `--bg`/`--surface` ≥ 4.5:1 (well above the
     normal palette's 3:1 non-text bar); `--danger` vs `--bg`/`--surface` ≥ 7:1. Apply the same set of
     assertions to both `hc-light` and `hc-dark`.
  4. Assert every semantic token name present under `:root[data-theme="light"]` is also present under
     both `:root[data-theme="hc-light"]` and `:root[data-theme="hc-dark"]` — no dropped token (same
     shape-check pattern as CPE-1539/CPE-1534).
- If a starting hex fails its threshold, adjust the primitive (not the semantic mapping) until the test
  passes — that's the authoring loop this ticket is doing.

## Verify
`npx vitest run src/app.css.hc-contrast.test.ts` (new contrast + completeness assertions, all passing);
`npm run check`. Fully headless — no GUI needed to land it, since nothing consumes
`data-theme="hc-light"`/`"hc-dark"` yet.

**Async visual sign-off queued (not headless):** whether the high-contrast palette *reads* as a coherent
high-contrast theme (not just passes contrast math) is subjective and cannot be judged headlessly. Once
CPE-1545/CPE-1546 wire it live, queue an attended visual pass for the user (side-by-side against normal
light/dark, a few representative surfaces: file list, a dialog, a menu, the tab strip) rather than
treating contrast compliance alone as "done" aesthetically. Land this ticket on contrast-test-green and
flag the aesthetic pass as outstanding in the Work Log.

## Notes
**Conflict surface:** `src/app.css` only (two new `:root[data-theme="hc-*"]` blocks, additive after the
existing dark block) plus one new test file. No `src/App.svelte`, `src/lib/theme.ts`,
`src/lib/settings.ts`, or `SettingsDialog.svelte` edits. **Dispatch order:** independent — can run in
parallel with CPE-1544 (no shared files); CPE-1545/CPE-1546 depend on this landing before the palette is
visually complete, but neither blocks on it for their own build/test (an unmatched `data-theme` value
just falls back to the bare `:root` block, same "inert" degrade CPE-1539 relied on).
