---
id: CPE-1648
title: "Two colour-blindness simulations disagree about the syntax-highlighting palette — settle it by looking, and pick one method for the crew"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
CPE-1631 (PR #834) gave the app its first working syntax-highlighting palette. Three agents independently
assessed it for colour-blind legibility and **produced materially different numbers**, because they used
different published dichromacy matrices. Nobody was wrong; the methods genuinely disagree on magnitude for
near-collision pairs. That means we currently cannot answer "is this palette safe?" with confidence, and the
next theming ticket will hit the same wall.

## The disagreement, on the record
- **Worker** (Machado et al. 2009 — the family Chrome DevTools uses): worst-case distance for any pair it
  touched went **1.4–6.0 → ≥24.8** across all themes and CVD types. It also caught and fixed a
  keyword-vs-title collision at 1.4 in hc-dark/protanopia that its own simulation surfaced.
- **Re-reviewer** (Viénot, Brettel & Mollon 1999): worst case ranges **~12–30**, not a uniform ≥24.8. Two
  specifics: the worst pair in several theme/CVD combinations is the **untouched** `string`-vs-`comment`
  (dark/deuteranopia: 13.3, unchanged before and after); and under this matrix family the `tag` re-hue moved
  `comment`-vs-`tag` in dark/protanopia from 39.1 to **12.1** — i.e. *worse*.
- An earlier **Visual Critic** (Brettel/Viénot, approximate) put light-theme `tag` at ~40–52 from title,
  string and comment — "a real, quantified risk", again a different picture.

WCAG luminance contrast is unaffected by any of this and was independently re-verified: all 24 token/theme
cells clear their bar (4.5:1 normal, 7:1 high-contrast) against both `--surface` and `--surface-alt`. And
three of the six buckets carry a **non-hue** cue already (keyword and title are bold, comment is italic), so
nothing here is a legibility emergency.

## What to actually do
1. **Settle this one palette by looking, not by matrix.** Use Chrome DevTools' own CVD emulation
   (Rendering → Emulate vision deficiencies) on real highlighted code — a real Rust file, a real TypeScript
   file, a real JSON file — in all four themes. Can you distinguish string from comment, and tag from title,
   with protanopia and deuteranopia emulated? That is the question the numbers were proxying for.
2. **If any pair genuinely collides, prefer a non-hue fix.** The durable answer isn't nudging hues until one
   matrix is happy — it's that only 3 of 6 buckets carry a non-hue cue. Separating string/number/tag by
   lightness, or giving one of them a weight or style difference, survives every simulation method.
3. **Then pick ONE method and write it down** in `.claude/qa-architecture/` so future theming work is
   comparable rather than re-arguing from scratch. Chrome DevTools' emulation is the pragmatic choice — it's
   what a developer can actually see, and it's the Machado family, so it matches the tool people will reach
   for.

## Acceptance criteria
- A recorded visual judgement (both themes × both common CVD types, real code) on whether any token pair is
  genuinely hard to tell apart, with screenshots in the work log.
- Any collision found is fixed, preferably with a non-hue cue.
- Contrast stays at or above the current values — re-measure, don't assume.
- One CVD method chosen and documented in the QA architecture notes for future tickets.

**Conflict surface:** `src/app.css` (`--hljs-*` tokens), `.claude/qa-architecture/`. Touches global theme
tokens — don't run alongside other theming work (CPE-1632 is the other one).
