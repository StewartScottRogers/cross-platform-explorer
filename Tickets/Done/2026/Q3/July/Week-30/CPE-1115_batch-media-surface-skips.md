---
id: CPE-1115
title: "Batch media: surface skipped files loudly (don't silently drop un-decodable inputs)"
type: bug
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-26
epic: CPE-723
---

## Summary
UX gap found by the user on the installed 0.57.36 build. Running Batch-media Compress over a 2-file selection
(`photo.jpg` + `pixel.png`) produced only ONE output — because `photo.jpg` was a degenerate/placeholder JPEG
(Exif header, no image data) that can't be decoded. The backend did the RIGHT thing (skip-on-error: skip the
un-processable file, still write the valid one — `execute_plan_walk` reports it in `BatchReport.skipped`), but
the app surfaced it too quietly, so the user experienced it as "a file went missing / it's broken."

## The fix (make skips loud + preventable)
1. **Completion notice** — after Apply, if `report.skipped.len() > 0`, show a PROMINENT, persistent notice
   listing the skipped files + their reasons (e.g. "photo.jpg — not a valid image"), not just a terse
   "N converted, M skipped" that's easy to miss. Consider a small skipped-list in the dialog before it closes.
2. **Live preview flag (better — prevent the surprise)** — in the `batch_media_plan` preview rows, flag inputs
   that will fail (e.g. an input the decoder can't open). This likely needs a lightweight per-file "can decode?"
   probe (a cheap header/dimensions check via the image crate, bounded) surfaced in the plan or a sibling
   command, so the row shows "⚠ can't process — will be skipped" BEFORE the user clicks Apply. Keep it bounded
   / no full decode if avoidable.
3. Verify the current dialog actually renders `report.skipped` at all (the completion summary path) — if it
   only shows a count, that's the immediate gap.

## Acceptance Criteria
- [ ] After a batch with skips, the user clearly sees which files were skipped and why (prominent, not a
      one-line count they can miss).
- [ ] (Stretch) The plan preview flags un-decodable inputs before Apply.
- [ ] `npm run check` clean; vitest green (skip-rendering + any probe helper tested); no new deps.

## Work Log
2026-07-26 — Filed from a real user report on 0.57.36: Compress over [degenerate photo.jpg + valid pixel.png]
wrote only pixel-out.png; the skip of photo.jpg (undecodable) wasn't surfaced clearly. Backend is correct
(skip-on-error); this is purely making the skip visible/preventable in the UI.

2026-07-26 — DONE (Foreman-built solo; crew at the 200-agent session cap). The dialog now HOLDS OPEN on a
results panel when a run has skips, listing every skipped file + reason ("photo.jpg — not a valid image") with
a "✓ N written · ⚠ M skipped" header and a "Done" button, instead of silently closing with only a first-skip
toast. Pure `skipRows` helper (basename+reason) added + unit-tested; component test proves hold-open + list +
Done-dispatch. Self-verified: `npm run check` 0/0, full suite 1216 green (incl. new tests), no new deps, theme
vars only. NOT run through the independent Reviewer+UAT gauntlet (agent cap) — low-risk additive frontend
change; eligible for a confirmatory review in a fresh session. The preview-side "flag undecodable inputs before
Apply" stretch goal is deferred (needs a per-file decode probe).
