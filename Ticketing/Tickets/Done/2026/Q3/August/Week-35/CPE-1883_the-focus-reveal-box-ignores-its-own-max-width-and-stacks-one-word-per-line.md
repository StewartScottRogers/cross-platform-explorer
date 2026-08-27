---
id: CPE-1883
title: the status bar's focus-reveal box ignores its own max-width and stacks one word per line
type: bug
priority: Low
status: Done
tags: ready
estimate: S
created: 2026-08-23
closed: 2026-08-27
---

## Problem

CPE-1833 made the status bar's advisory notes reachable by keyboard: the pills got `tabindex="0"`
and a `:focus-visible` rule that reveals the untruncated sentence in a box over the row. The rule
declares `max-width: min(90vw, 420px)`, which reads as "a normal few-line tooltip".

It does not render as one. Measured by PR #1019's independent UAT in real Chrome:

| Case | focused width | focused height |
|---|---|---|
| 600px, compound busy row | 58px → **64px** | 16px → **148px** |
| 900px, uncrowded | 151px → **157px** | 16px → **52px** |

The box grows **downward, not outward**, rendering one or two words per line:

> their / names / could not / be shown / safely

**Cause, diagnosed in the same pass:** the `:focus-visible` rule never overrides the pill's flex
sizing (`flex: 0 var(--priority-shrink) auto` stays in force), so the flex algorithm keeps squeezing
the item toward its shrink-allocated width and `max-width` never gets to act as anything but an
unreached ceiling.

Note it reproduces at 900px in an uncrowded bar, so this is **not** the narrow-width or compound-state
case — it is the reveal itself.

## Severity, stated honestly

Low, and it is not an accessibility failure. Nothing is hidden, clipped mid-word, or lost:

- the DOM text is always the full sentence (verified via `textContent`)
- the screen-reader live region carries it correctly (CPE-1833's actual acceptance criterion —
  verified present, `ignored: false`, `live=polite`, `atomic=true`, with the right combined text)
- the `title` tooltip still works on hover

So the letter of the AC is met. What is wrong is that a **sighted keyboard user** gets a tall, ugly
word-column instead of a readable box, which looks broken rather than deliberate.

## What to do

Likely a one- or two-line fix: override the flex sizing inside the `:focus-visible` rule —
`flex-shrink: 0`, or `flex-basis: auto` — so `max-width` can take effect. **Verify, do not assume**:
the UAT's diagnosis is well-evidenced but was not tested as a fix.

**Measure it the way the finding was measured.** `scripts/dev-harness/statusbar-notice/` runs under
plain `chrome.exe --headless=new` — no WebDriver, nothing to install — and reports element rects. Take
before/after width and height at **both** 600px compound-busy and 900px uncrowded, and put the numbers
in the work log. A screenshot of the focused pill at each is worth more than any assertion.

Guard it if CPE-1882 has landed by then (the ticket that wires this harness into CI): a rect assertion
that the focused box is wider than it is tall would catch this exact regression and is trivially
red-proofable by reverting the fix.

## Acceptance criteria

- [x] The focused reveal renders as a readable box, not a word-per-line column, at 600px and 900px.
- [x] Before/after rect measurements recorded at both widths.
- [x] Nothing regresses for the screen-reader path — the live region still carries the combined text
      atomically.
- [x] The full sentence is still reachable by hover, focus and AT.

## Work Log

- **2026-08-23 19:00 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`,
  from PR #1019's UAT, which measured it in real Chrome, diagnosed the flex-shrink cause, and judged
  it non-blocking. I agree: the PR merged. This is the cosmetic remainder.

- **2026-08-27 — Worker, branch `cpe-1883-focus-reveal-maxwidth`.** Reused
  `scripts/dev-harness/statusbar-notice/` (plain `chrome.exe --headless=new` over raw CDP, no
  WebDriver) exactly as directed, wired into the CI-blocking `scripts/dev-harness/layout-guard` job
  (CPE-1882 landed first). Added `?focus=filtered-hidden|unreadable` (+ `?fh=`/`?un=` count overrides,
  `?theme=`) to the harness so it can programmatically focus a pill before measuring; a real headless
  tab is never OS-focused, so `Emulation.setFocusEmulationEnabled` (CDP) had to be added to
  `engine.mjs` first or `:focus-visible` never matched at all (`document.hasFocus()` was `false`).

  **Diagnosis confirmed, ticket's numbers reproduced almost exactly** (busy compound row, both
  widths — my harness measures the SAME props state at both widths, unlike the ticket's two-scenario
  table, and still lands on the ticket's own numbers within font-rendering rounding):

  | Case | width (before → after) | height (before → after) |
  |---|---|---|
  | 600px, compound busy row | 63.9px → 367.3px | 148px → 16px |
  | 900px, compound busy row | 157.0px → 367.3px | 52px → 16px |

  **Three fixes attempted, two rejected — all three measured, not assumed:**
  1. `flex-shrink: 0` on `:focus-visible` — stops the column, but gives the pill real flex-row width at
     the worst possible moment, squeezing `.git`/`.disk` (later SHRINKS-FIRST priority) to **`width: 0`**
     at 600px busy. Rejected before commit.
  2. `position: absolute` directly on the span — removes it from flex layout (no squeeze at all), but
     Chromium's static-position computation for an absolutely-positioned flex CHILD does not reproduce
     its in-flow location: measured jumping to `left: 0` (the row's own padding edge), covering
     `.item-count`/`.selected-count`/`.dim` instead of overlaying rightward as designed. Rejected.
  3. **Shipped fix**: the reveal moved to a `::after` pseudo-element, anchored via `position: absolute;
     left: 0; top: 0;` relative to the SPAN itself (`position: relative`, unconditional, never resized by
     focus — a stable containing block, not a flex child's ambiguous static position) with `content:
     attr(data-reveal)` (new `data-reveal` attribute, since generated content can't read text-node
     children) and — the actually load-bearing declaration — **`width: max-content`**: without it, an
     abspos box with `width: auto` and `left: 0` still computes shrink-to-fit WITHIN its containing
     block's remaining space (CSS2.1 §10.3.7), and the containing block here is the narrow, still
     flex-shrunk span — reproducing the identical one-word-per-line column one level down. `width:
     max-content` is not `auto`, so that containing-block-constrained algorithm never runs.

  **Neighbour impact, measured at 600px busy (the worst case) — confirms zero regression:**

  | Element | unfocused | focused (shipped fix) |
  |---|---|---|
  | `.git` | left=459.6 w=79.6 | left=459.6 w=79.6 (identical) |
  | `.disk` | left=565.2 w=20.8 | left=565.2 w=20.8 (identical) |
  | `.item-count` | left=14.0 w=77.2 | left=14.0 w=77.2 (identical) |

  The span's own box (and therefore the row's flex layout) is never touched by any of this — the reveal
  is purely an overlay.

  **CI guard added**: `scripts/dev-harness/layout-guard/cases.mjs` gained a `statusbar-focus-reveal`
  case (widths 600/900, `busy=1&focus=filtered-hidden`), and `engine.mjs` gained a new `rectBounds`
  check kind (`maxHeight`/`minWidth`, optionally against a `pseudo: "::after"` box via
  `getComputedStyle` — pseudo-elements have no `getBoundingClientRect()`). Red-proofed twice: against
  the original unfixed CSS (148px height, `BOUNDS` violation) and again by stashing just the shipped
  fix (`data-reveal`/`::after` both absent → 0×0 → `minWidth` violation, "reveal silently stopped
  rendering" case) — both went red, both go green with the fix restored. `npm run harness:layout-guard`
  passes clean (14/14) with the fix in.

  **Known narrow edge case, left open (out of this ticket's AC, documented in the CSS comment):** at
  the app's 600px width floor WITH the full compound-busy row, `.filtered-hidden` sits far enough right
  that even the fixed 367px-wide box runs ~100px past the viewport edge — `body { overflow: hidden }`
  (app.css) clips it (no scrollbar), so the sentence's tail is invisible in that one specific
  combination only. Every other measured combination (900px busy, either width uncrowded) stays fully
  on-screen. Not a regression from this fix's own baseline (the pre-fix column also started at the same
  left position, just never grew wide enough to reach the edge) — flagged for the Foreman to triage
  (new low-priority ticket vs. accepted limitation), not fixed here (needs viewport-aware JS
  positioning, not a CSS-only change, and no AC covers it).

  `npm run check`: 0 errors/warnings. `StatusBar.a11y.test.ts` (9 tests, live-region/AT coverage) and
  the other 4 `StatusBar.*.test.ts` files (34 more tests): all green, unchanged. Screenshots (600px,
  busy row, both themes) at `.claude/sprint-metrics/visual-evidence/cpe-1883-{light,dark}-{before,
  after}.png`.
