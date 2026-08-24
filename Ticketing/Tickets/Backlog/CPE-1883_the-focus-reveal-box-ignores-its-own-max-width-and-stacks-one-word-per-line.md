---
id: CPE-1883
title: the status bar's focus-reveal box ignores its own max-width and stacks one word per line
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-23
closed:
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

- [ ] The focused reveal renders as a readable box, not a word-per-line column, at 600px and 900px.
- [ ] Before/after rect measurements recorded at both widths.
- [ ] Nothing regresses for the screen-reader path — the live region still carries the combined text
      atomically.
- [ ] The full sentence is still reachable by hover, focus and AT.

## Work Log

- **2026-08-23 19:00 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`,
  from PR #1019's UAT, which measured it in real Chrome, diagnosed the flex-shrink cause, and judged
  it non-blocking. I agree: the PR merged. This is the cosmetic remainder.
