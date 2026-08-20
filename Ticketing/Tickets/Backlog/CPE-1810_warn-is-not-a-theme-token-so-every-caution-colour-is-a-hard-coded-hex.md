---
id: CPE-1810
title: "--warn is not a theme token, so every caution colour in the app is a hard-coded hex"
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-20
closed:
---

## Problem

`--warn` is referenced as a CSS custom property in a number of components — but **it is not defined
anywhere in `src/`.** Every one of those call sites is really `var(--warn, <hex>)`, so the fallback
*always* wins and the "caution" colour is a hard-coded hex in disguise.

The codebase already knows this and says so in two places:

- `src/app.css.solid-fill-contrast.test.ts:237-241` — resolves a literal fallback "when the token itself
  isn't defined anywhere in that theme (e.g. `--warn` — components that lean on a CSS custom-property
  fallback instead of a real theme token)".
- `src/lib/components/AgentTimeline.svelte:2102-2105` — calls it **"this file's *older* `var(--warn, <hex>)`
  fallback idiom"**, contrasts it with "real, always-defined semantic vars", and says to use those,
  "never a hard-coded hex".

So the deprecation is documented; the migration was never done.

## Why it matters

A fixed hex renders identically in light and dark. This app ships a real dark theme (CPE-1492/1493) with a
WCAG contrast guard, and the amber values in use (`#b5872b`, `#b8860b`) are precisely the case that guard
exists for. The result is that a **warning is least legible in the theme where it most needs to be seen** —
and the contrast guard cannot catch it, because the value never passes through a token it inspects.

There is also a slow ratchet effect: each of these sites is a hard-coded hex occurrence, and the repo's
`BASELINE_TOTAL_HEX_OCCURRENCES` guard only ratchets down. Every new caution-coloured element copied from
an existing one pushes against that guard rather than with it.

## What to do

- **Define `--warn` as a real semantic token in *both* the light and dark palette blocks**, with values
  chosen to pass the WCAG contrast guard in each. Show the contrast numbers; do not eyeball them.
- Then migrate the existing `var(--warn, <hex>)` call sites and **delete the now-dead fallbacks**. A
  half-migration is worse than none — it leaves readers unable to tell which sites are live.
- Ratchet `BASELINE_TOTAL_HEX_OCCURRENCES` **down** by the number of literals removed, so the guard records
  the improvement rather than merely tolerating it.
- Check whether the same shape exists for other undefined tokens; `--warn` was found by accident, so
  assume it is not alone. Grep for `var(--` with a fallback and cross-check each name against the palette
  blocks.

## Notes

Filed by the Foreman during the batched sprint, 2026-08-20. Found when CPE-1803's fix reached for the
"caution" idiom, hit the hard-coded-hex ratchet, and bumped the baseline to get past it — the guard was
right and the idiom was wrong. That PR took the correct narrow route (real tokens, box treatment instead of
hue, baseline restored); this ticket is the general fix it declined to take on as scope creep.

Related: **CPE-1803**, **CPE-1492/1493** (the dark theme and its contrast guard).

## Work Log

**2026-08-20** — Implemented on branch `cpe-1810-warn-theme-token`.

- Defined `--warn` (general foreground/border/tinted-background token) and a sibling `--warn-fill`
  (solid-fill-background-with-white-text role, ConsentSheet's `.badge` — the same two-role tension
  CPE-1632/CPE-1649 already found for `--danger`/`--danger-fill`) in **all four** live theme blocks —
  light, dark, hc-light, hc-dark (the ticket asked for light+dark; hc-light/hc-dark were added too
  because they're wired live via `theme.ts`, and deleting the fallback without a value there would
  have made the caution colour silently disappear in high-contrast mode instead of just being wrong).
  Contrast numbers (WCAG 2.1, computed not eyeballed):
  - light: `--warn`/`--warn-fill` = `#8a5a00` (reused `--pal-amber-700`) — 5.93:1 vs `--surface`
    (white); no role tension since `--surface` IS literal white there.
  - dark: new `--pal-dark-amber-500: #c38800` — fgSurf 4.61:1 (>=4.5 AA body text), fgBg 5.31:1,
    white-on-fill 3.07:1 (>=3 WCAG 1.4.11 UI floor). One value serves both roles (positive but
    narrow luminance window, ~[0.283, 0.30]) — no split needed, unlike hc-dark below.
  - hc-light: new `--pal-hc-light-amber-900: #734900` — 7.83:1 vs white (clears the AAA-inspired
    7:1 bar); trivial, same reasoning as light.
  - hc-dark: negative window (7:1 fgSurf needs relative luminance >=0.3282; white-on-fill 3:1 needs
    <=0.30 — same non-overlapping gap CPE-1649 found for `--danger`), so split into
    `--pal-hc-dark-amber-300: #ffcc66` (13.03:1 vs surface, text-role only) and
    `--pal-hc-dark-amber-fill: #bd8300` (white-on-fill 3.28:1, fill-role only).
- Migrated every real `var(--warn, <hex>)` / `var(--warn, <hex>)`-as-background call site
  (AgentTimeline.svelte x10, ConsentSheet.svelte x2, ExplorerPane.svelte x1, ImageCompareView.svelte
  x3, SidecarManager.svelte x8 = 24 total) onto the real tokens and deleted the fallbacks — 24 hex
  literals removed, not the 22 I first hand-counted (AgentTimeline had 10 call sites, not 8; missed
  two on first pass). Updated the two doc comments that referenced the old undefined-token idiom by
  name (AgentTimeline's `.hd-unclean-note` comment, TrashView's `.tv-degraded-note` comment) so they
  no longer claim `--warn` is undefined.
- Ratcheted `src/app.css.test.ts`'s `BASELINE_FILES_WITH_HEX` 90->87 and
  `BASELINE_TOTAL_HEX_OCCURRENCES` 466->442 (actual pre-fix count was 88/466 — the files-with-hex
  baseline already had 2 free of slack before this ticket). ImageCompareView.svelte dropped out of
  the "has hex" set entirely — its only hex literals were `--warn` fallbacks.
- Added `src/app.css.warn-token.test.ts`: (a) asserts `--warn`/`--warn-fill` resolve to a concrete
  hex in all 5 theme blocks (bare `:root` default + light/dark/hc-light/hc-dark) as a HARD failure —
  deliberately not reusing the existing solid-fill-contrast guards' resolution, which silently SKIPS
  an unresolvable pairing instead of failing (documented there as "nothing to assert against
  statically"), which is exactly the blind spot that let `--warn` go undefined for years; (b) greps
  every `.svelte` file (comments stripped) for a `var(--warn`/`var(--warn-fill` call site that still
  carries a fallback, guarding against a future half-migration. Verified both assertions go red:
  removing hc-dark's `--warn` line makes test (a) fail with `.toMatch() expects to receive a string,
  but got undefined`; reintroducing `var(--warn, #d08b2b)` in SidecarManager makes test (b) fail
  listing that file as an offender — and separately makes the *existing* ratchet test in
  `src/app.css.test.ts` fail too (`expected 443 to be less than or equal to 442`), confirming the
  tightened baseline actually bites. Both reverted after observing red.
- **"Same shape" check (requested, not fixed)**: grepped every `.svelte` + `app.css` for
  `var(--token, <fallback>)` and cross-referenced each token name against everything `app.css`
  actually defines. Found several more undefined-token-with-hex-fallback sites of the identical
  shape, left out of scope for this ticket (fix exactly what was asked; no drive-by refactors) —
  flagging for a follow-up ticket:
  - `--text-muted` (fallback `#9a9a9a`) — AgentTimeline.svelte (~17 sites), ConsultedFiles.svelte
    (2), FileList.svelte (1). By far the biggest offender — same shape as `--warn` was.
  - `--accent-2` (fallback `#209764`) — BackupDashboard.svelte, 1 site.
  - `--bg-dim` (fallback `#0f0f0f`) — SidecarManager.svelte, 1 site.
  - Lower-priority/likely-benign, not colour/WCAG-relevant: `--mono` (font-family fallback, ~20
    files) is a different risk category entirely (no contrast implication). `--agent-accent`,
    `--accent-soft`, `--surface-2` all fall back to *another real, always-defined token* (e.g.
    `var(--agent-accent, var(--agent-unknown))`), not a raw hex, so they're not the same bug — the
    fallback there is an intentional default, not a disguised hard-coded colour.
- Gates: `npm run check` — 0 errors, 0 warnings. `npx vitest run` (full suite) — **317 test files,
  4193 tests passed** (4182 pre-existing + 11 new in `app.css.warn-token.test.ts`). No Rust touched,
  so no `cargo clippy` run.
- Not user-facing in the CPE-579 sense (no new Section/control — a colour-correctness bug fix to
  existing UI), so no `src/docs/*.md`/`sectionDocs.ts` change.
