---
id: CPE-1661
title: Syntax highlighting — keyword and title collide under colour blindness in the light themes, with no non-colour cue to fall back on
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

Found by the independent Visual Critic on PR #850, by pixel-sampling the CVD screenshots CPE-1648 produced
and then re-checking the **rendered code panels** (not just the legend swatches).

In the **light** and **hc-light** themes, under protanopia and deuteranopia, `keyword` (`fn`, `let`,
`struct`) and `title` (`Manifest`, `Path`, `Result`, `String`) converge into the same indigo-blue family:

| Category | Normal vision | Protanopia |
|----------|---------------|------------|
| `keyword` | magenta | `#434a9a` |
| `title`   | blue     | `#3d56b3` |

Close in all three channels — two shades of the same blue where a normal-vision reader sees two clearly
different hues.

**What makes this one different from the rest of the palette.** Every other CVD-tight pair in this palette
is rescued by a typographic cue: `comment` is italic, `tag` is plain, so even though keyword/comment/tag all
collapse to one grey family in the dark themes, the rendered code stays unambiguous. That does not work
here — **keyword and title are both bold**. The bold cue only separates each of them from ordinary code; it
does not discriminate between the two of them. So this pair has colour, and nothing else.

## Why it matters

Roughly 1 in 12 men has some form of red-green colour vision deficiency. For those readers, in the two light
themes, the distinction between a language keyword and a type/function name is currently carried entirely by
two hues that CVD renders nearly the same.

## Scope

Give the pair a second, non-colour channel, or move the hues apart — one of:

1. **Stop bolding `title`** (keep `keyword` bold). Cheapest, uses the cue that already works elsewhere in
   this palette, and matches what several mainstream editor themes do.
2. **Shift `title`'s light-theme hue** further from `keyword`'s so the pair survives the Machado projection
   — must be re-verified under simulation, not just picked by eye under normal vision.
3. A different weight/style split, if either of the above costs too much visually under normal vision.

Constraints:
- Semantic tokens only, never a hard-coded hex; any new token defined in **both** the light and dark blocks
  (the WCAG guard test enforces this).
- The existing `hljs-contrast` guard must stay green in all four themes.
- Do not disturb the dark/hc-dark treatment, which is already correct.

## Acceptance criteria

- [ ] Under protanopia **and** deuteranopia simulation, `keyword` and `title` are distinguishable in
      `light` and `hc-light` — by hue **or** by a non-colour channel, not by lightness alone.
- [ ] The dark and hc-dark themes are unchanged.
- [ ] `app.css.hljs-contrast.test.ts` stays green in all four themes.
- [ ] A test pins the result: apply the Machado 2009 matrices (the standard CPE-1648 settled, recorded in
      `.claude/qa-architecture/CVD-SIMULATION-METHOD.md`) and assert either a minimum perceptual distance
      between the pair **or** that they differ in weight/style. Then break it deliberately and watch the
      test go red — a guard nothing can fail is not a guard.
- [ ] While here, add a guard for the **load-bearing cue** the same review surfaced: `comment` must stay
      italic and `keyword` must stay bold. Those cues are currently the only thing separating
      keyword/comment/tag in the dark themes, and today nothing would notice if someone removed them.

## Notes

Filed by the Foreman from the PR #850 Visual Critic report, 2026-08-12. **PR #850 was not blocked on this**:
its CPE-1649 half (high-contrast solid-fill contrast) is a separate, correct, independently-verified fix,
and this collision is **pre-existing** — no colour in this PR moved it. What #850 did have to fix before
merging was the *record*: its CPE-1648 decision document claimed no genuine collision existed and named
`string` vs `tag` as the closest pair, which the pixel data contradicts. That correction, and the warning
that bold/italic are part of the accessibility contract, landed with #850.

A separate, subjective item from the same review went to the user's async queue rather than into this
ticket: whether hc-dark's newly-darkened accent/danger fills are *vivid* enough, given that the contrast fix
both darkens and desaturates them.
