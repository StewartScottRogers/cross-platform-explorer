---
id: CPE-1649
title: "hc-dark's solid-fill buttons/badges fail contrast WORSE than normal dark theme — the theme built for accessibility is the least accessible one"
type: Bug
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found while widening the contrast guard for CPE-1632 (white-on-solid-`--danger`/`--accent` failing in the
normal dark theme). The same white-on-solid-fill pattern — `.btn.primary`/`.btn.primary.danger`,
`.agent-badge.removed`/`.tl-badge.removed`, etc. — also renders in the **high-contrast** theme
(`:root[data-theme="hc-dark"]`, CPE-1543/epic CPE-1496), since those components just reference `--accent`/
`--danger`, whatever theme is active. `theme.ts` shows OS high-contrast IS wired live (`queryOsHighContrast()`
selects `hc-light`/`hc-dark` at runtime) — this isn't the "inert until CPE-1544/1545/1546" state some of
app.css's older comments still describe.

Measured (white text on the hc-dark solid-fill tokens, same WCAG math CPE-1632 used):
- white on hc-dark `--danger` (#ff8080) = **2.43:1**
- white on hc-dark `--danger-hover` (#ffb3b3) = **1.70:1**
- white on hc-dark `--accent` (#66b3ff) = **2.22:1**
- white on hc-dark `--accent-hover` (#99ccff) = **1.69:1**

All four are under WCAG's 3:1 UI-component floor — and **worse** than the normal dark theme's pre-CPE-1632
numbers (2.88:1 / 2.28:1 / 2.59:1 / 1.80:1). The theme a user turns on specifically to get MORE contrast
renders its primary/destructive buttons LESS legibly than the default theme did. hc-light does not have this
problem (white on hc-light `--danger`/`--accent` measure 10.01:1 / 7.79:1 — both comfortably clear).

## Scope note
Deliberately NOT fixed as part of CPE-1632, which scoped itself to the normal light/dark theme pair (see that
ticket's "Conflict surface" note) and whose guard extension (`src/app.css.solid-fill-contrast.test.ts`) only
resolves tokens through the light/dark palettes, not hc-light/hc-dark. The hc themes are their own epic
(CPE-1496) with their own guard file (`src/app.css.hc-contrast.test.ts`, CPE-1543) — this bug belongs there.

## Fix
- Darken hc-dark's `--pal-hc-dark-red-300`/`--pal-hc-dark-red-200` (danger/danger-hover) and
  `--pal-hc-dark-blue-300`/`--pal-hc-dark-blue-200` (accent/accent-hover) until white text on each clears
  3:1, the same two-role tension CPE-1632 solved for the normal dark palette (foreground-text role vs.
  solid-fill-background role) — reuse that approach and its worked numbers as a starting point.
- Re-verify hc-dark still clears its OWN stricter AAA-inspired foreground bar
  (`src/app.css.hc-contrast.test.ts`: text/danger/success >=7:1, dim/UI >=4.5:1) after the change — the
  narrower-luminance-window trade CPE-1632 hit for the normal dark theme may be tighter here since hc-dark's
  floor is higher.
- Extend `src/app.css.hc-contrast.test.ts` (or add a sibling file mirroring CPE-1632's
  `solid-fill-contrast.test.ts`, resolving through the hc-light/hc-dark palettes instead) with the same
  usage-derived white-on-solid-fill scan, so this class of bug can't recur silently in the high-contrast
  themes either.
- Negative control: confirm the new guard fails against today's hc-dark values before fixing them.

## Acceptance criteria
- White text on every hc-dark solid-fill token (danger, danger-hover, accent, accent-hover) clears 3:1.
- hc-dark's existing AAA-inspired foreground assertions still pass.
- A guard test covers the hc-theme solid-fill pairing, with a negative-control result recorded in the work log.

**Conflict surface:** `src/app.css` (hc-dark palette block), `src/app.css.hc-contrast.test.ts`. Touches global
theme tokens — don't run alongside other theming work.

## Work Log

- 2026-08-11 (sprint Worker) — **Key discovery: the ticket's suggested fix (darken the SAME
  `--pal-hc-dark-red-300`/`-200` primitives) is mathematically impossible for `--danger`.**
  hc-dark's own AAA-inspired text bar requires `--danger` >= 7:1 against `--surface` (#0d0d0d),
  which needs relative luminance >= 0.3282; the white-on-solid-fill 3:1 UI floor needs relative
  luminance <= 0.30. Those ranges don't overlap (a -0.028-wide negative window) — unlike the normal
  dark theme's positive-but-narrow window CPE-1632 found (bar is only 4.5:1 there) or `--accent`
  here (bar is only 4.5:1, giving a real [0.193, 0.30] window). Verified numerically before touching
  any CSS (see the ticket's own measured numbers above, reproduced): white-on-#ff8080 = 2.43:1,
  white-on-#66b3ff = 2.22:1 — both confirmed, plus the negative-window proof.
  - **Fix actually shipped:** `--accent`/`--accent-hover` got the single-token darken the ticket
    suggested (`--pal-hc-dark-blue-300` #66b3ff→#4c85be, `--pal-hc-dark-blue-200` #99ccff→#5590cc) —
    feasible because accent's own bar is only 4.5:1. `--danger` was left **unchanged** (#ff8080,
    already 8.01:1 vs `--surface` — it was never the problem) and a **new semantic token**,
    `--danger-fill`, was split off to carry the solid-fill-background role instead (defaults to
    `var(--danger)` in light/dark/hc-light — zero visual change there; only hc-dark points it at a
    new primitive, `--pal-hc-dark-red-fill: #be6060`). `--danger-hover` had no separate text-role
    consumer anywhere in the app (grepped every `var(--danger-hover)` — all 4 were solid-fill
    backgrounds), so it was simply darkened in place (#ffb3b3→#a65454), no split needed.
  - 12 component files' `.btn.primary.danger`/`.agent-badge.removed`/`.tl-badge.removed`/
    `.agent-chip.removed`/`.status.bad`/etc. repointed from `var(--danger)` to `var(--danger-fill)`
    for their `background`(+matching `border-color`); decorative non-text usages (`.fill.err`,
    `.drive-bar-fill.full`, `.state-dot.state-error`) left on `--danger` unchanged (no white text
    ever paints on them).
  - New guard: `src/app.css.hc-solid-fill-contrast.test.ts` — CPE-1632's usage-derived scanner,
    resolved through hc-light/hc-dark instead of light/dark. Had to add one real fix mid-build: the
    scanner's "white foreground" detection had to exclude `--text`/`--text-dim`/`--text-faint` (the
    general body-text roles) — in hc-dark those legitimately resolve to literal `#ffffff` by design
    (that's what "high contrast" means), which without the exclusion made ~200 ordinary
    `hover:{background:X; color:var(--text)}` rules across the app look like deliberate solid-fill
    buttons to the scanner. Documented in the guard file itself.
  - **Negative control (red→green), reproduced by temporarily reverting the palette values and
    re-running the new guard:** RED — 4 failures, exactly the ticket's 4 broken pairs: white on
    var(--accent)=#66b3ff [hc-dark] = 2.22:1, white on var(--accent-hover)=#99ccff [hc-dark] =
    1.69:1, white on var(--danger-fill)=#ff8080 [hc-dark] = 2.43:1, white on
    var(--danger-hover)=#ffb3b3 [hc-dark] = 1.70:1 (all < 3:1). After restoring the fix: GREEN, all
    16 tests pass (`npx vitest run src/app.css.hc-solid-fill-contrast.test.ts`).
  - `src/app.css.solid-fill-contrast.test.ts`'s own regression-pin sanity check needed a one-line
    update (`--danger` → `--danger-fill` in its `tokenPairings.has(...)` assertion) since the real
    component consumers it scans for moved token names — its WCAG assertions themselves are
    unaffected (light/dark/hc-light all still resolve `--danger-fill` to the same value as
    `--danger`).
  - Full verification: `npm run check` clean; `npx vitest run` — 293 files / 3761 tests, all green
    (includes all 7 contrast/WCAG guard files: `app.css.test.ts`, `app.css.light-contrast.test.ts`,
    `app.css.dark-contrast.test.ts`, `app.css.hc-contrast.test.ts` — hc-dark's own AAA-inspired
    foreground bars still pass post-fix — `app.css.hljs-contrast.test.ts`,
    `app.css.solid-fill-contrast.test.ts`, `app.css.hc-solid-fill-contrast.test.ts`).
  - PR opens alongside CPE-1648 (same branch/PR, per the sprint assignment). Not marked Done here —
    Foreman does that after independent review + UAT + Visual Critic.
