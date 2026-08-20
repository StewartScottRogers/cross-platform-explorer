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
