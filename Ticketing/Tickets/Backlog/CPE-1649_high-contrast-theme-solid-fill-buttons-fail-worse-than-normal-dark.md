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
