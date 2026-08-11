---
id: CPE-1631
title: "Syntax highlighting is dead app-wide — highlight.js emits hljs-* classes but no stylesheet ever defines them, so every code view renders flat monochrome"
type: Bug
status: Doing
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

## Work Log

2026-08-11 — Added six token-based `--hljs-*` colours (`--hljs-keyword`/`-title`/`-string`/`-comment`/
`-number`/`-tag`) to `src/app.css`, defined in ALL FIVE theme blocks (bare `:root` fallback, light, dark,
hc-light, hc-dark — not just light+dark; hc blocks get the same AAA-inspired treatment every other hc
token gets), each resolving through a new `--pal-*`/`--pal-dark-*`/`--pal-hc-light-*`/`--pal-hc-dark-*`
primitive (no hard-coded hex in the semantic layer). Added the actual `.hljs-*` global CSS rules
(grouped the same way highlight.js's own `a11y-light`/`a11y-dark` reference themes group their classes —
see `node_modules/highlight.js/styles/a11y-light.css` — just re-expressed through these tokens instead of
hard-coded hex) — this was the missing piece; `highlight.ts` was already correctly emitting `hljs-*`
markup, nothing ever styled it.

**Measured contrast ratios** (WCAG relative-luminance formula, verified by the new guard test below —
numbers below are vs `--surface`; the guard test also checks `--surface-alt` and both clear the same
bar with only marginal difference since the two backgrounds are close in luminance in both palettes):

| token | light (need ≥4.5:1) | dark (need ≥4.5:1) | hc-light (need ≥7:1) | hc-dark (need ≥7:1) |
|---|---|---|---|---|
| `--hljs-keyword` | 7.59:1 | 7.27:1 | 9.30:1 | 13.14:1 |
| `--hljs-title` | 5.67:1 | 6.82:1 | 10.28:1 | 12.20:1 |
| `--hljs-string` | 5.08:1 | 9.22:1 | 8.20:1 | 14.86:1 |
| `--hljs-comment` | 6.11:1 | 5.88:1 | 9.56:1 | 12.24:1 |
| `--hljs-number` | 7.39:1 | 7.63:1 | 9.47:1 | 13.02:1 |
| `--hljs-tag` | 6.29:1 | 7.93:1 | 9.36:1 | 13.77:1 |

All six tokens clear WCAG AA (light/dark) and the hc palette's AAA-inspired bar with real margin — no
token is anywhere near its threshold. Added `src/app.css.hljs-contrast.test.ts` (10 tests, mirroring
`src/app.css.dark-contrast.test.ts`/`src/app.css.hc-contrast.test.ts`'s existing pattern) to assert this
mechanically and catch any future drift.

**Verified by looking, in real Chrome, in all four theme variants** (jsdom cannot see colour, so this
could not be trusted to a passing test alone). Built a small reusable harness —
`scripts/dev-harness/hljs-theme/` + `vite.harness.hljs-theme.config.ts` + `npm run harness:hljs-theme`
— that runs the REAL `highlight.ts` against a representative TypeScript sample (plain code preview,
`PreviewPane.svelte`'s `.code-view`/`.cl-code` markup) and a Python sample (notebook code cell,
`NotebookPreview.svelte`'s `.nb-cell`/`.nb-code` markup) with the app's real `src/app.css` loaded,
switchable via `?theme=`. Confirmed in Chrome DevTools/screenshots: keywords/purple, titles/blue,
strings/green, comments/dim-italic-gray, numbers/rust, tags/teal — clearly distinguishable and legible
against `--surface` in light, dark, hc-light, and hc-dark (hc themes toggled via
`document.documentElement.dataset.theme` in the console, since no runtime wires `hc-*` live yet — same
caveat every other hc-theme ticket in this repo carries). No console errors. Adapted (rather than reused
verbatim) the CPE-1635 `checkpoint-narrow` harness's pattern per its own header comment's invitation —
this check needed neither its iframe/`vw` trick (nothing here is viewport-width-relative) nor its
invoke/bindings mocks (`highlight.ts` has no Tauri dependency), so it's a flatter, simpler harness on
its own port (4320) reusing the same "own tiny page + real `app.css` + `data-theme` query param"
convention.

Added a `gui-smoke` capture too (CPE-1629's preview-pane screenshot suite had landed the night before):
`preview-pane.smoke.ts` gained one more `it()` that opens the already-seeded `CPE-1096-fixture.rs`
fixture (real Rust source, already used by `open-dir.smoke.ts` to prove the code-preview's
outline/rows/minimap render — reused here rather than adding a new fixture), asserts a real `hljs-`
class actually landed in the DOM (not just escaped plain text — the CPE-1631 bug's exact failure mode
would pass a weaker "some span exists" check), and captures both a dark-narrow and a light-wide
screenshot. This was cheap: it's the same navigate → click → wait → `snap()` recipe every other test in
that file already follows, reusing an existing fixture and existing helpers — no new sample file, no new
navigation primitive.

`npm run check`: 0 errors, 0 warnings. `npx vitest run`: **284 test files, 3498 tests, all green** —
this total already includes the one new file this ticket adds (`app.css.hljs-contrast.test.ts`, 10
tests); no pre-existing test was touched or weakened. `gui-smoke`'s new spec was type-checked
(`npx tsc --noEmit` in `gui-smoke/`, 0 errors) but NOT run end-to-end — that suite drives a real
`tauri build` output over WebDriver, out of scope to spin up for this ticket; it follows an
already-proven pattern in the same file closely enough that this is a reasonable risk to accept, and
CI will run it for real on the PR.

No `src/docs/*.md` change: `src/docs/03-explorer.md` already documents "syntax highlighting when a
language is recognised" as existing preview behaviour (line ~62) — this ticket makes that existing claim
true, it doesn't add a new user-facing section, so CPE-579's doc-registry trigger doesn't apply.
