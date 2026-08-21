---
id: CPE-1833
title: the status bar's advisory notes are never announced, and truncate into a title attribute only
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

The status bar carries advisory sentences that change what the listing above it *means* — "N hidden by
filter", and (from CPE-1780) "N entries could not be read". Two accessibility gaps, both measured
during the CPE-1780 visual review:

1. **Never announced.** Neither `.statusbar` nor either note carries `role="status"`, `aria-live`, or
   any other live-region marker (measured: `barRole: null`, `barAriaLive: null` at every width). A
   screen-reader user is never told that what they are reading is filtered or incomplete — which is
   the whole point of the notes.

   **The naive fix does not work.** Each note is `{#if count > 0}`-conditionally mounted, i.e. it
   appears as a NEW element already containing its final text. That is precisely the shape that is
   frequently *not* announced, even with `role="status"` on the span itself. Chromium plus Windows AT
   — WebView2 with NVDA or Narrator, exactly this app — is the weakest pairing for it. The same lesson
   was learned on CPE-1816 the same day: a live region must exist *before* its content changes.

2. **The full sentence is reachable only by mouse.** At 684px and 600px both notes truncate
   (`isClipped: true`) and the complete text survives only in the `title` attribute. `title` is
   hover-only — not reliably exposed to keyboard-only or screen-reader users — so at a narrow window a
   low-vision or keyboard user has no path to the count at all. 600x400 is the app's own
   `.min_inner_size`, so this is a supported size, not an edge case.

## Acceptance criteria

- [ ] A **persistent, always-mounted** container wraps the advisory region with `role="status"` /
      `aria-live="polite"`, and the notes change its text content rather than being inserted and
      removed. Verify the mid-change announcement actually happens, do not infer it from the markup.
- [ ] Both notes are covered, and the case where **both apply at once** announces sensibly rather than
      as two competing sentences.
- [ ] The full sentence is reachable without a mouse when the text is visually truncated — an
      accessible name that carries the untruncated text, or another affordance. `title` alone is not
      sufficient and must not be the only path.
- [ ] Verified with a real screen reader against the installed build. This is a behaviour of the
      assistive technology, not of the DOM — the markup already looks fine, which is the point.
- [ ] Nothing regresses visually: the notes still truncate with an ellipsis and the row still fits at
      600px.

## Notes

Found by the Visual Critic during the CPE-1780 review, which classified both as follow-ups rather than
merge blockers — they are pre-existing for the filter note, and CPE-1780 doubles the surface rather
than introducing the gap.

Closely related and worth doing together if either is picked up: **CPE-1828** is the identical
mounted-with-its-text live-region defect in the Trash view's degraded notes. Same root cause, same
fix shape, same verification problem. Whoever takes one should take both.

The real-screen-reader verification step is a candidate row for the QA Architect's manual-test burndown
if it cannot be automated.
