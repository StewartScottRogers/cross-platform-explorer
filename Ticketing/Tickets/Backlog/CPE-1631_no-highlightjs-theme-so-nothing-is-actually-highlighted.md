---
id: CPE-1631
title: "Syntax highlighting is dead app-wide — highlight.js emits hljs-* classes but no stylesheet ever defines them, so every code view renders flat monochrome"
type: Bug
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found by the Visual Critic reviewing CPE-1616 (PR #822) in real Chrome — the kind of defect that is invisible
to the test suite and was only ever going to be caught by looking. The notebook viewer's PR summary said
"code cells syntax-highlighted via `highlight.ts`", and the code genuinely calls into highlight.js. But on
screen, every code cell rendered as **flat monochrome monospace with zero colour variation**, in both light
and dark theme, at every width.

## The gap
`highlight.ts`'s `highlightCode()` / `highlightForFile()` do their job: they run highlight.js and emit markup
decorated with `hljs-*` class names (`hljs-keyword`, `hljs-string`, `hljs-comment`, …).

**Nothing in the app ever defines those classes.** A repo-wide grep for `.hljs-` across `src/` returns no
rule, and there is no highlight.js theme stylesheet imported anywhere. The classes land in the DOM and style
nothing.

This is **not** a notebook regression — it is a pre-existing, app-wide gap. Every surface that routes through
`highlight.ts` is affected, including the plain code/text preview. The notebook viewer simply made it
visible, because a notebook is mostly code.

## Why it matters
It is a silent, total failure of a shipped feature: the work of parsing and classifying every token is being
done on every preview, and then thrown away. So it costs performance *and* delivers nothing — the worst of
both against PURPOSE.md's fast/small/predictable tiebreaker. Users looking at code in this app have never
seen it highlighted.

## Fix
Ship a highlight.js theme and make sure it is actually applied:
- Prefer **defining the `hljs-*` rules in the app's own CSS using the existing semantic theme tokens**, rather
  than importing a stock third-party theme. The app has a real light AND dark theme, and a stock theme is
  tuned for one background — the Visual Critic's standing warning is that a highlight palette tuned for light
  goes muddy on dark. Token-based rules stay correct in both automatically.
- If a new token is introduced for a token colour, it must be defined in **BOTH** the light and dark blocks —
  there is a WCAG contrast guard test that enforces this.
- Verify the result **by looking at it in a real browser in both themes**, not by a passing test. jsdom cannot
  see colour any more than it can see layout.
- Check contrast for each token colour against its background in both themes; code is small text, so the AA
  threshold is 4.5:1.

## Acceptance criteria
- A code preview and a notebook code cell both render with visible, correct highlighting in **both** themes.
- Every colour comes from a semantic token; no hard-coded hex; new tokens defined in both blocks.
- Screenshot evidence in the work log (both themes), since no automated test can assert this today —
  and if CPE-1629 has landed preview-pane screenshot specs by then, add a spec that covers a highlighted
  code cell so this can never silently die again.
- Measured contrast ratios for the token palette in both themes, meeting AA.

**Conflict surface:** `src/app.css` (or wherever theme tokens live), possibly `src/lib/preview/highlight.ts`.
Touches global CSS — do not run in parallel with other theming work.
