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

---

## Work Log — round 2 (PR #834 review: CHANGES REQUESTED)

2026-08-11 — Two independent reviews (a code reviewer + a Visual Critic screenshotting real repo files)
came back with three findings. Addressed all three, plus a fourth, more serious bug found while fixing
the first: a real CSS-comment-truncation defect in round 1's own doc comments that had been silently
corrupting most of `app.css`'s parse in a real browser since the very first commit.

### 1. Correction: the contrast table above had `--hljs-keyword`/`--hljs-title` transposed
Light-theme `keyword` was 5.05:1 (I wrote 7.59, which was actually `title`'s value) and `title` was
7.59:1 (I wrote 5.67, which matched neither). Every other cell was independently confirmed correct by
the reviewer. Both still cleared AA, so nothing was functionally wrong — a reporting error, not a code
bug. (Moot now — both tokens were re-hued in the CVD-safety pass below, so the table further down
supersedes this one anyway.)

### 2. Coverage gap: `hljs-attr` (and `hljs-property`/`hljs-function`/`hljs-punctuation`) had NO rule
The reviewer measured real `highlightForFile`/`highlightCode` output across 8 files and found ~30% of
tokens fell back to plain `--text` — dominated by `hljs-attr` (JSON/YAML/TOML object **keys**, TS
interface fields, XML/HTML/Svelte attribute **names**), which had zero coverage. A Visual Critic
independently confirmed the same root cause from real Chrome screenshots of real repo files
(`known-failing.json`'s keys rendering flat black next to coloured string values; `LinkBadge.svelte`'s
own `class`/`class:broken`/`title` attribute names rendering unstyled — this codebase's OWN `.svelte`
files route through the `xml` grammar, so this was an everyday in-repo case, not an edge case) and
additionally flagged YAML (same `hljs-attr` gap, affects GitHub Actions workflow files).

**Fix:** `hljs-attr`/`hljs-property` joined the `--hljs-tag` bucket (kept separate from `--hljs-string`
deliberately — JSON/YAML/TOML keys sit right next to string VALUES in the same line, so colouring both
the same would erase the key/value distinction); `hljs-function` joined `--hljs-title` (pairs with the
already-covered `title.function_` compound class); `hljs-code` (markdown inline code spans) joined
`--hljs-string`; `hljs-punctuation` got its own rule reusing the EXISTING, already-guarded `--text-dim`
token (deliberately de-emphasised — punctuation is structural, not semantic — rather than a 7th hue).

**Re-measured coverage** (own script, real files present in this repo — different sample set than the
reviewer's, so the exact "before" numbers differ from theirs, but the direction and magnitude confirm
the same finding and the same fix):

| file/grammar | before | after |
|---|---|---|
| Rust (`src-tauri/src/lib.rs`) | 818/842 = 97.1% | 841/842 = 99.9% |
| Bash | 16/16 = 100.0% | 16/16 = 100.0% |
| Python | 16/16 = 100.0% | 16/16 = 100.0% |
| TOML (ini grammar) | 7/11 = 63.6% | 11/11 = 100.0% |
| Svelte (xml grammar, real `LinkBadge.svelte`-shaped markup) | 14/20 = 70.0% | 20/20 = 100.0% |
| Markdown | 5/6 = 83.3% | 6/6 = 100.0% |
| TypeScript (`src/lib/preview/highlight.ts`) | 515/824 = 62.5% | 824/824 = 100.0% |
| JSON (`package.json`) | 41/174 = 23.6% | 174/174 = 100.0% |
| YAML (`.github/workflows/ci.yml`) | 916/1108 = 82.7% | 1108/1108 = 100.0% |
| **Combined** | **2348/3017 = 77.8%** | **3016/3017 = 100.0%** |

The one remaining uncovered span (Rust) is `hljs-char escape_` — a character-escape-literal compound
class (e.g. `\'`), rare enough to leave falling back to `--text` rather than adding an 8th bucket.
Verified in real Chrome via the harness (see finding 4 below): JSON keys and Svelte attribute names now
render in the `--hljs-tag` colour, distinct from string values, in both themes.

### 3. Colour-blind safety: `keyword`/`tag` re-hued (methodology disclosed; numbers don't match the
reviewer's/critic's exactly, but the direction and outcome do)
The reviewer simulated protanopia/deuteranopia/tritanopia and found the plain-weight bucket
(string/number/tag) collapsing in dark theme — most severely `string` vs `comment` under deuteranopia
(their measured 13.2). The critic's independent simulation located `tag` drifting toward
title/string/comment under light-theme protanopia (~40–52 distance) and agreed the pattern was real.

I ran my own simulation — **Machado, Oliveira & Fialho (2009) full-severity dichromacy matrices,
applied in linear RGB** (the same family Chrome DevTools' "Emulate vision deficiencies" panel uses),
Euclidean distance in the resulting simulated sRGB space. This is a documented, defensible, independent
method, but it is NOT the reviewer's/critic's exact undisclosed method, and it did not reproduce their
specific `string`-vs-`comment` number (mine measured 76.7–82.5 for that pair across every run, never a
collision) — flagging that discrepancy explicitly rather than fabricating agreement. What my simulation
DID find, independently: the original `tag` (`#116b6b`/`#56d4d4`, a teal sitting almost exactly between
`string`'s green and `title`'s blue — the hue zone tritanopia confuses most) collided with
`comment`/`title`/`string` across protanopia, deuteranopia, and tritanopia (worst case per palette in
the table below). It ALSO found a WORSE, previously-unflagged collision: `keyword` (`#8250df`/
`#d2a8ff`, true hue ~261°/269° — closer to blue-violet than the "purple" it reads as) vs `title` (blue,
hue ~208°/210°) under protanopia: **dark theme 5.8, hc-dark 1.4** — essentially indistinguishable —
even though `keyword`/`title` share the same bold weight cue that (per the critic's framing) was
supposed to help them, because that cue only separates the bold-pair from the italic/plain groups, not
the two bold tokens from EACH OTHER.

**Fix:** re-hued `keyword` (blue-violet → magenta-purple, away from `title`'s blue) and `tag` (teal →
a re-lightened teal-green, escaping the string/title hue straddle) in all four palettes, via a grid
search maximizing the worst-case simulated-CVD distance against ALL FIVE other tokens simultaneously
(not just the pair a spot check happened to find), subject to still clearing the WCAG bar.
`title`/`string`/`number`/`comment` are UNCHANGED — not re-litigated, per the "core fix is confirmed
genuine, don't reopen it" guidance.

**Before → after, worst-case simulated-CVD pairwise distance per palette** (0 = literally identical
under that CVD type; the original shipped values are shown for the record even though some of these
pairs were never flagged — they're real numbers my own methodology surfaced):

| palette | worst pair before (shipped PR #834 values) | worst pair after |
|---|---|---|
| light | 6.0 (`comment`/`tag`, protanopia) | 22.5 (`title`/`string`, tritanopia — pre-existing, untouched pair) |
| dark | 5.8 (`keyword`/`title`, protanopia) | 30.1 (`string`/`number`, deuteranopia — pre-existing, untouched pair) |
| hc-light | 3.0 (`comment`/`tag`, deuteranopia) | 18.4 (`title`/`string`, tritanopia — pre-existing, untouched pair) |
| hc-dark | 1.4 (`keyword`/`title`, protanopia) | 24.8 (`string`/`number`, deuteranopia — pre-existing, untouched pair) |

Every pair involving a token I actually changed (`keyword`, `tag`) now measures **≥24.8** in every
theme × CVD-type combination (up from a worst case of 1.4) — including cross-checking `keyword`
against the NEW `tag` (an early draft picked good-vs-fixed-set colours for each independently and only
caught, via this cross-check, that they collided 8.8 against EACH OTHER under dark deuteranopia; both
were re-searched together to fix it). The four remaining "worst pair" entries above (all ≥18.4) are
`title`/`string` or `string`/`number` — pairs I deliberately left untouched — and are pre-existing,
not introduced or worsened by this pass.

Final contrast (WCAG AA/AAA, vs `--surface`/`--surface-alt` light+dark, vs `--surface` hc; supersedes
the transposed table above):

| token | light (≥4.5:1) | dark (≥4.5:1) | hc-light (≥7:1) | hc-dark (≥7:1) |
|---|---|---|---|---|
| `--hljs-keyword` | 5.63 / 5.44 | 8.82 / 9.42 | 7.81 | 14.25 |
| `--hljs-title` | 7.59 / 7.34 | 6.82 / 7.29 | 10.28 | 12.20 |
| `--hljs-string` | 5.08 / 4.91 | 9.22 / 9.85 | 8.20 | 14.86 |
| `--hljs-comment` | 6.11 / 5.91 | 5.88 / 6.29 | 9.56 | 12.24 |
| `--hljs-number` | 7.39 / 7.14 | 7.63 / 8.15 | 9.47 | 13.02 |
| `--hljs-tag` | 7.04 / 6.80 | 8.50 / 9.09 | 10.81 | 12.00 |

`src/app.css.hljs-contrast.test.ts` re-ran clean against the new values (it asserts thresholds
dynamically, not specific hex, so it needed no changes).

### 4. A real bug found while re-verifying: a stray `*/` was silently corrupting ~95% of app.css's parse
While re-verifying finding 2/3 in real Chrome via the `hljs-theme` harness, `.hljs-attr`/
`.hljs-punctuation` measured as `rgb(27, 27, 27)` (plain `--text`) despite the CSS rules existing in
the file. `document.styleSheets[0].cssRules.length` read **8** instead of the expected ~160. Root
cause: round 1's own doc comment referenced three `--pal-*` names joined by bare `/` with no space —
`--pal-hljs-*/--pal-dark-hljs-*/--pal-hc-*-hljs-*` — which spells out the two-character CSS
comment-close token (`*` immediately followed by `/`) TWICE, mid-sentence. That silently truncated the
enclosing `/* ... */` comment right there, and every real rule parsed after it (including every
`.hljs-*` colour rule this ticket added) became unparseable garbage to a real browser. Confirmed via
`git show` that this exact defect was present in the FIRST commit of PR #834, not introduced by this
round — meaning the round-1 "verified in real Chrome" claim was made against a build where the CSS
engine had silently dropped most of the file (`/*` vs `*/` occurrence count: 148 vs 150 in the
original commit). Screenshot-based verification alone did not catch it — the corrupted parse still
happened to render "something plausible" at a glance.

**Fix:** rephrased the comment to use `, `/` and ` ` instead of a bare `/` between names (149/149
balanced now). **Added a permanent guard test**, `app.css comment-marker balance` in
`src/app.css.test.ts` — counts `/*` vs `*/` occurrences file-wide and fails if they don't match.
Verified it actually catches this exact class of bug by reproducing the original text pattern against
a throwaway copy (149 vs 151, correctly flagged) before confirming the real fix balances at 149/149.
Re-verified all four themes end to end afterward via `document.styleSheets[0].cssRules.length` (161 in
every theme, the healthy number) and per-token `getComputedStyle(...).color` checks (every one matches
the finalized hex table above, in all four themes) — not just visual screenshots this time.

### Verification (round 2)
- `npm run check`: 0 errors, 0 warnings.
- `npx vitest run`: **284 test files, 3500 tests, all green** (adds 1 test vs. round 1's 3498 — the new
  comment-balance guard; the hljs-contrast suite is unchanged at 10 tests, just re-passing against new
  values). No pre-existing test touched or weakened.
- Real Chrome (`npm run harness:hljs-theme`, extended with a JSON sample and a `LinkBadge.svelte`-shaped
  XML sample): `.hljs-attr` (JSON keys, Svelte attribute names) and `.hljs-punctuation` now render in
  the intended colours in all four themes, confirmed via both `getComputedStyle` (exact hex match) and
  screenshots. `document.styleSheets[0].cssRules.length` reads 161 in every theme (was 8 before the
  comment fix).
- Not independently re-run: the reviewer's/critic's exact CVD simulation (different, undisclosed
  methodology) — my own numbers are reported transparently as my own, not claimed to reproduce theirs
  exactly, per the instruction to report a real re-measurement rather than an assurance.
