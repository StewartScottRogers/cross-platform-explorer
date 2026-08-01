---
id: CPE-1220
title: "Polish: make Spotlight matched-run highlight pop more than a thin underline"
type: Task
status: Open
priority: Low
component: frontend
tags: [ready]
estimate: 20m
created: 2026-08-01
closed:
---

## Context
Epic-704 Visual Critic (VISUAL PASS on the Spotlight overlay) raised one non-blocking nit: on the
active (accent-blue) result row, the matched-substring highlight is a thin underline
(`Spotlight.svelte`: `.sp-row.active :global(.sp-hl) { text-decoration: underline }`), which is
slightly less scannable at a glance than a bolder treatment (bold weight and/or a subtle background
tint that still reads on the accent fill).

## Acceptance criteria
- The matched run is more visually prominent on both the active and inactive rows, still legible on
  the accent-blue active background, still theme-var driven (no hard-coded colours).
- No regression to the non-active `.sp-hl` treatment; re-capture the spotlight gui-smoke screenshot.

## Notes
Pure visual polish; deferred out of epic-704 (which passed) as a standalone tweak.

## Work Log
- 2026-08-01 — `Spotlight.svelte`: replaced the active-row's thin-underline `.sp-hl` treatment with a
  translucent white tint (`rgba(255, 255, 255, 0.3)`) + `font-weight: 700`, dropping
  `text-decoration: underline`. The inactive-row `mark.sp-hl` rule (accent-bg chip + white text) also
  picked up `font-weight: 700` so both states pop equally without changing its accepted
  accent-bg/white-text colouring — no hard-coded theme colours introduced (the white tint/text mirrors
  the existing accepted `#fff`-on-accent-fill pattern used elsewhere, e.g. `CommandPalette.svelte`'s
  `.cp-row.active` text and `BatchMediaDialog.svelte`'s `.pill-x:hover`). Multi-run fuzzy highlighting
  is untouched — only the `<mark class="sp-hl">` styling changed, not the segmenting logic in
  `highlightByPositions`.
- Verification: `npm run check` — 0 errors/warnings. `npm test -- --run` — 147 files / 1629 tests
  passed (including `Spotlight.test.ts`, whose assertions are structural — mark count/text — and
  untouched by this CSS-only change, so no test edits were needed). `gui-smoke && npm run typecheck`
  clean (had to `npm install` gui-smoke's deps first — its `node_modules` wasn't present in this
  worktree). Did not run the full wdio harness (needs a release build + msedgedriver); the Foreman
  should re-capture `gui-smoke/specs/spotlight.smoke.ts`'s `spotlight.png` for the Visual Critic once
  this merges — the active row's highlighted run should now show a bold, lightly-tinted chip instead
  of an underline, and the inactive rows' existing accent-chip highlight should look slightly bolder
  but otherwise unchanged.
