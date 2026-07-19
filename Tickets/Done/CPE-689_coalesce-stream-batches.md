---
id: CPE-689
title: Coalesce stream batches to stop the per-batch full re-sort
type: enhancement
component: Frontend
priority: high
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-688
estimate: 1h
---

## Summary
First (safe, headless) child of CPE-688. Today `loadPath` appends each 256-row stream batch to `entries`
individually, and the reactive `visible = sortEntries(entries…)` re-sorts the whole growing array on every
batch (plus every downstream derivation recomputes) — ~N/256 full sorts over a growing array. Buffer the
incoming batches and flush them **once per animation frame**, so the re-sort collapses to ~one per frame.
First batch still paints immediately (streaming-liveness / STREAMING.md preserved).

## Acceptance Criteria
- [x] Stream batches are buffered and flushed to `entries` on a rAF, not per-onmessage.
- [x] First rows still appear promptly (loading placeholder clears on the first flush); final settle-flush
      guarantees no buffered rows are dropped when the stream ends or is superseded.
- [x] Superseded (mid-navigation) streams drop their buffer; final order unchanged.
- [x] `npm run check` + full suite green (the App integration tests still render all rows).

## Work Log
2026-07-18 (dayshift) — Picked up as CPE-688's safe first child. No questions; best-guess.

## Resolution
`loadPath` (src/App.svelte) now buffers incoming stream batches and flushes them to `entries` once per
`requestAnimationFrame`, instead of `entries = entries.concat(batch)` on every onmessage. The reactive
`visible` re-sort (and downstream derivations) therefore runs ~once per frame rather than ~N/256 times over
a growing array — the headline O(n²)-ish cost from CPE-688's diagnosis #2. The first frame's rows still
paint immediately (loading placeholder clears on the first flush), preserving streaming-liveness; a
synchronous settle-flush after the walk completes guarantees no buffered rows are dropped (and covers
test/no-rAF environments); a superseded stream drops its buffer. check clean; full suite green (678); build
clean. Live large-folder timing improvement is best confirmed on the installed build. Files: src/App.svelte.
