---
id: CPE-1888
title: the Drop Stack reserve is a magic number matched to today's handle, and nothing tests text scaling
type: task
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-25
closed:
---

## Problem

CPE-1884 fixed the Drop Stack handle covering the Sidebar's bottom rows by reserving 50px at the
bottom of `.navigation-pane` (`src/app.css`). It works, and PR #1024's UAT confirmed it works at every
window height it tested, with real dispatched mouse clicks, 14 of 14.

But **the reserve is a static number matched to the handle's current rendered size**, and the two live
in different files with nothing tying them together: `.handle-btn` is `height: 28px` at
`bottom: 14px` in `DropStackPanel.svelte`; the `50px` is in `src/app.css`. Change one and the bug comes
back silently.

The UAT quantified exactly how much slack there is, which is the useful part:

| Vector tried | Result |
|---|---|
| Longer label / 40-char locale string | handle grows **wider only** (113px → 312px), height unchanged — `white-space: nowrap` absorbs it |
| Count badge ("999") | no height change — sits inline in the flex row |
| Browser page zoom (`Emulation.setPageScaleFactor(3.0)`) | **immune by construction** — scales handle and reserve together |
| **Root font-size scaling** (OS "larger text" style) | **breaks it.** Baseline handle 28px leaves 34px slack; reserve exhausted once the handle exceeds **~62px**. 250% held with +9px spare; **300% broke it, −2px** |

At that point the same silent click-interception returns — and, per the UAT's own reproduction on
`main`, "silent" undersells it: at the 600×400 floor a click aimed at "Reset section order" **activated
the Drop Stack handle instead**. Wrong control, not just no control.

## Why this is Medium and why CPE-1884 shipped anyway

Windows caps text scaling around **225%**, and 250% still held with room to spare — so the failing
range is beyond what the platform that surfaced the bug can reach. Weighed against leaving a **live,
reachable** bug unfixed at every ordinary window height, merging was clearly right. The Foreman
overrode the UAT's `FAIL` on those grounds, and the UAT explicitly framed it as a judgement call
rather than a defect ("so the Foreman can judge whether the residual risk is acceptable to ship anyway
or worth a fast-follow"). This ticket is that fast-follow.

## What to do

1. **Stop hard-coding the reserve.** Derive it from the handle's real rendered height — a
   `ResizeObserver` on `.handle-btn` writing a CSS custom property, or a shared token both files
   consume — so it cannot drift from the thing it is reserving space for. Prefer whichever survives a
   future redesign that makes the handle taller, since that is the realistic failure, not a user at
   300% text.
2. **Cover text scaling in the guard.** `scripts/dev-harness/sidebar-drop-stack-overlap/check.mjs`
   sweeps window heights only, so this whole regression class is invisible to it. Add a root
   `font-size` axis — at minimum baseline, 150%, 225% (the Windows cap) and one beyond it — and assert
   the containment invariant at each. The UAT's numbers give you the expected pass/fail boundary to
   red-proof against.
3. **While in there**, the reviewer on PR #1024 noted the per-height loop in `check.mjs` has no
   per-iteration `try/catch`, so one browser crash aborts the whole sweep instead of reporting the
   other heights. Still fails loud; just less useful. Cheap to fix alongside.
4. If **CPE-1882** (wiring the real-browser layout harness into CI) has landed by then, this guard and
   its new axis belong there rather than as a manual script.

## Acceptance criteria

- [ ] The reserve is derived from the handle's actual size, not a literal.
- [ ] Making the handle taller in `DropStackPanel.svelte` alone does **not** reopen the overlap —
      demonstrated.
- [ ] The guard sweeps text scaling as well as window height, and red-proofs at a known-bad setting.
- [ ] A single browser failure mid-sweep no longer aborts the remaining heights.

## Work Log

- **2026-08-25 14:20 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`, from
  PR #1024's UAT. That UAT wrote its own CDP harness rather than re-running the author's, reproduced
  the original bug with real dispatched clicks (finding the worse-than-described symptom along the
  way), confirmed the fix at 14 heights, judged the 50px of dead space acceptable from a user's seat
  with screenshots, and *then* found the one vector that outgrows the reserve — with the crossover
  measured to within 2px rather than asserted.
