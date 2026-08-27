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

- **2026-08-27 (round 2) — Worker, same branch.** Visual Critic + Reviewer UAT on PR #1045 both
  fetched the shipped CSS and re-rendered it themselves in real headless Chrome, independent of my
  screenshots. Findings and fixes, all measured before/after — commits before probing each time:

  1. **Screenshot staleness (Critic blocker 1).** My round-1 "after" screenshots turned out to show a
     REJECTED intermediate attempt (`position: absolute` directly on the span, before the final `::after`
     pseudo-element design), not the shipped commit — a file-path/ordering slip while iterating through
     three fix attempts and re-shooting screenshots each time, not a defect in the shipped code itself
     (confirmed: the Critic's own from-source re-render matched my Work Log's numbers, not the stale
     screenshots'). All four screenshots recaptured this round from the actual current source, with the
     embedded harness diag inside each image cross-checked against the measured table below before
     treating them as done.

  2. **600px viewport overflow — real, and worse than the original bug (Critic blocker 2, confirmed by
     Reviewer).** The shipped `left: 0` anchor grows the box RIGHTWARD; at the app's 600px width floor
     with the full compound-busy row, that ran the box ~100px past the viewport edge, and
     `body { overflow: hidden }` (app.css) silently clipped the sentence's TAIL — no ellipsis, no scroll,
     no cue. Visible text: "4 entries were hidden because their names could" — "not be shown safely" simply
     gone. Worse than the pre-fix column, which showed every word. **Fix**: `right: 0; left: auto`
     (unconditional) — the pill always sits in the right half of a busy row, so the box now grows
     LEFTWARD from the pill's right edge, which is always on-screen.
     Re-measured, both widths, `scripts/dev-harness/layout-guard` (`statusbar-focus-reveal` case):

     | width | box span | viewport | on-screen? |
     |---|---|---|---|
     | 600px busy | left=25.7 right=393.0 | [0, 600] | yes, fully |
     | 900px busy | left=126.4 right=493.8 | [0, 900] | yes, fully |

     Did NOT file a follow-up ticket for the clipping — the Foreman's instruction was to fix it outright
     with `right: 0`, and it is now measured fully on-screen at both tested widths, so there is nothing
     left to track.

  3. **Vertical alignment (Critic).** Measured pre-fix: hairline top y=4.5, bottom y=25.5 in a 26px bar —
     bottom-flush, shadow dying into the window edge. Cause: `top: 0` anchored to the pill's 16px text
     box, then this rule's own 2px padding + 1px ring pushed the visible box 5px down. **Fix**:
     `top: 50%; transform: translateY(-50%);` — centres the box in the bar.

  4. **Dark-theme contrast (Critic).** No `color` here meant the reveal inherited the PILL's
     `var(--accent)` — dark `#0078e0` on `--surface #2b2b2b` measured 3.21:1 for a full sentence of 12px
     body text, under the 4.5:1 AA floor (light was 5.5:1, fine). `--accent` is correctly a non-text
     accent by design (`app.css.dark-contrast.test.ts:270` only asserts >=3:1 on purpose — focus
     rings/icons, not body text), so this is a **guard blind spot**, same family as **CPE-1919** and
     **CPE-1921** (already filed) — colour-as-text uses the guard has no reason to catch. Not a bug in
     the guard; a gap in what it was ever asked to cover. **Fix**: `color: var(--text)` on the `::after`
     only — the pill underneath keeps `--accent`/`--warn`, correct for its own short truncated label.

  5. **Click-swallow on `.git`'s Pull/Push/Sync buttons (Critic + independently, Reviewer's own CDP
     hit-test probe).** `::after` is part of its originating element for hit-testing, so the reveal (up
     to 367px wide) could sit over those buttons while focused and absorb their first click — confirmed
     by the Reviewer at multiple widths, not only 600px as first assumed. **Fix**: `pointer-events: none`
     on both `:focus-visible::after` rules. Re-verified via a real-Chrome hit-test sweep
     (`document.elementsFromPoint` over each `.git-btn` centre at 900px busy): all three buttons are
     topmost/reachable with the fix in place.

  6. **New defect found capturing THIS round's own evidence, not flagged by either reviewer**: with
     `right: 0` anchoring the overlay leftward while the base span's own (now-unclipped, `overflow:
     visible`) raw text still flows rightward as always, the two no longer occupy the same horizontal
     range — the span's raw text bled out to the right of the opaque overlay, visible as a second,
     ellipsis-less copy of the sentence in the pill's own colour. **Fix**: `color: transparent` on the
     base `:focus-visible` rule (span only; the `::after`'s own explicit `color: var(--text)` is
     unaffected — pseudo-element colour is not "inherited transparency" once explicitly set). Safe for
     AT the same way every other colour-only-hiding technique in this file already is: the DOM text node,
     the accessible name computed from it, and the separate always-mounted `.sr-only` live region are
     untouched by `color`.

  7. **Red-proof narrative correction (Reviewer point #1)**: my round-1 Work Log claimed reverting the
     WHOLE `::after` block reproduces "the documented 148px stacked column" — it does fail the guard, but
     via the `0x0 / minWidth` "reveal vanished" path (the check is hard-wired to `pseudo: "::after"`,
     which the pre-CPE-1883 rule doesn't have), not the actual stacked-column shape. Re-ran correctly
     this time: removing ONLY `width: max-content` (keeping the rest of the `::after` architecture)
     reproduces the real failure mode — `height=160.0px exceeds maxHeight=90px` **and**
     `width=50.4px is below minWidth=100px` at 600px busy, both violations, matching the Reviewer's own
     numbers exactly. Restored `width: max-content`, reran clean.

  **CI guard extended**, not just re-verified — a new `pseudoOnScreen` check kind
  (`scripts/dev-harness/layout-guard/engine.mjs`), added specifically to catch class 2 above happening
  again. Went through two of its own corrections, each found by red-proofing it rather than trusting the
  first version:
    - v1 computed the pseudo's on-screen position from the check's own `edge` config field instead of
      measuring it, so reverting the CSS anchor didn't change what got measured — it could never have
      caught the regression it exists for. Fixed to read `getComputedStyle`'s actual resolved
      `left`/`right`.
    - v2 then assumed only ONE of computed `left`/`right` would resolve to a definite number — false:
      BOTH resolve to definite numbers for a fully-determined absolutely-positioned box (the un-authored
      side is algebraically derived, drifted ~8px from the authored anchor in testing). Fixed to trust
      whichever side computes to (near) zero, since this CSS pattern always authors its anchor as an
      exact `0` offset.
  Both corrections were themselves found by red-proofing: reverting `right: 0` back to `left: 0` and
  confirming the check actually goes red (it now does, at both widths — 600px shows a real viewport
  overflow violation AND an anchor-direction mismatch; 900px shows the anchor-direction mismatch even
  though that width happens not to overflow).

  Also documented but not changed: `Emulation.setFocusEmulationEnabled` (engine.mjs) is enabled once
  globally, not per-case — confirmed safe today (grepped every harness page, nothing else calls
  `.focus()`), but noted as a standing caveat for a future autofocusing case. And an open, honestly
  unexamined question in the CSS comment: `::after` generated content isn't a text node, so whether a
  sighted mouse user can still drag-select the sentence (with the real span's text now sitting under an
  opaque, non-drag-selectable overlay) is browser-dependent and untested — not a regression (pre-fix
  there was no readable box to select from either), just unverified either way.

  **Final verification**: `npm run check` clean. `npm run harness:layout-guard` 14/14 clean, including
  the new `pseudoOnScreen` check at both widths. All 43 `StatusBar.*.test.ts` tests green, unchanged.
  Screenshots recaptured from the actual final source and spot-verified two ways: visually (embedded
  harness diag readout inside each PNG cross-checked against the table above) AND by decoding the raw
  PNG pixel bytes directly (bypassing any viewer-side rendering) at several points inside the reveal box
  — dark theme reads `[43, 43, 43]` (`--surface` dark), light reads `[255, 255, 255]`, both exactly
  matching `getComputedStyle`'s own reported background for each theme.


- **2026-08-27 (round 3) — Worker, same branch.** One remaining defect, found by the Reviewer via a
  method neither of us had used yet: an actual dispatched CDP mouse click.

  **The bug**: round 2's `overflow: visible; color: transparent;` on the base `:focus-visible` rule
  (added to stop the span's raw text bleeding out beside the `::after` overlay) left that same raw
  text still PAINTING — invisibly, at zero alpha, `white-space: nowrap`, no width constraint — across
  its full unclipped ~367px natural width, with default `pointer-events: auto`. That invisible text
  physically overlaps `.git`'s Pull/Push/Sync buttons. A real click there landed on `.filtered-hidden`
  (the span), not the button: `{"pullClicked": false, "fhClicked": true}`, reproducible across trials.
  Arguably worse than round 1's version: there is now no visual cue at all that the click won't land.

  **Which API lies here, recorded so nobody reaches for it on this exact question again**: both
  `document.elementFromPoint` (singular) and `document.elementsFromPoint` (plural) reported the git
  buttons as reachable at this exact point — I used the plural form for round 2's own verification and
  got a clean, reassuring, WRONG answer; `selfPaint` (the existing check kind, built for CPE-1827) also
  uses the singular form and would have been equally fooled had I reached for it here instead of
  writing a bespoke check. Confirmed independently by the Reviewer with the SAME plural API, before it
  dispatched a real click and got the opposite answer. The failure shape is specific: an INVISIBLE,
  UNCLIPPED, `pointer-events: auto` element overlapping a control. For ordinary z-index/overlap
  regressions elsewhere in this repo, `elementFromPoint` is still the right tool (it is exactly what
  CPE-1836's `clipProbe` and CPE-1827/CPE-1884's `selfPaint` are built on, and nothing here calls that
  into question) — but for "does an invisible sibling still eat the click", only a REAL dispatched
  input event proved reliable. Two hit-test APIs agreeing with each other is not independent
  confirmation when both share the same underlying (and, for this one shape, wrong) hit-test path.

  **Fix**: `pointer-events: none` added to the BASE `.filtered-hidden:focus-visible,
  .unreadable:focus-visible` rule (previously only the `::after` overlay had it). Keyboard
  focusability is unaffected — `tabindex` focus does not route through `pointer-events`.

  **Verified with a real dispatched click**, not a hit-test API — matches the Reviewer's own method
  and exact coordinates:

  | width | click target | before (span text still `auto`) | after (`pointer-events: none` added) |
  |---|---|---|---|
  | 600px busy | `.git .git-btn` at (538.4, 13.5) | `clicked=false hit=SPAN.filtered-hidden` | `clicked=true hit=BUTTON.git-btn` |
  | 900px busy | `.git .git-btn` at (691.6, 13.5) | `clicked=false hit=SPAN.filtered-hidden` | `clicked=true hit=BUTTON.git-btn` |

  **CI guard, the third check kind built for this ticket**: new `clickReaches` kind
  (`scripts/dev-harness/layout-guard/engine.mjs`'s `runClickReachesChecks`), deliberately implemented
  OUTSIDE the in-page probe string every other check kind runs through — it needs CDP's own Input
  domain (`Input.dispatchMouseEvent`: mouseMoved -> mousePressed -> mouseReleased, a click synthesized
  through the renderer's real event pipeline), not anything reachable from browser JS. A capture-phase
  `click` listener installed fresh per target records the REAL event's `e.target`, which is what the
  check compares against, not a hit-test API result. Added to the `statusbar-focus-reveal` case
  (`{ kind: "clickReaches", selectors: [".git .git-btn"] }`), both widths. Red-proofed by removing just
  the new `pointer-events: none` line: both widths correctly go red with
  `CLICK-MISS clickReaches: ... landed on SPAN.filtered-hidden instead`, matching the manual table
  above exactly; restored, both go green (`clicked=true hit=BUTTON.git-btn`).

  **Final verification**: `npm run check` clean. `npm run harness:layout-guard` 14/14 clean, including
  the new `clickReaches` check at both widths. All 43 `StatusBar.*.test.ts` tests green, unchanged.
