---
id: CPE-688
title: "EPIC: Explorer performance — 10× faster directory open & all file-list operations"
type: Task
status: Proposed
priority: High
component: Frontend
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-18
---

## Summary
Opening a folder in the app takes **multiple seconds** where Windows Explorer is near-instant on the
same directory. The target: **10× faster** on directory open and on all file-list operations (scroll,
sort, select-all, filter, view-switch). The backend listing is already lean and streamed; the cost is on
the **frontend render pipeline**, and the single biggest cause is that the file list is **not
virtualized** — it builds a DOM node for every entry in the folder.

## Goal
A folder that opens in ~2–4s today opens in **well under half a second** to first interactive paint, and
scrolling/sorting/selecting stays smooth (60fps) regardless of folder size — the way Explorer behaves. No
regression to the small-folder common case, and the plain explorer stays fast/small/predictable
(PURPOSE.md tiebreaker).

## Diagnosis (measured from the code, 2026-07-18)
Investigated the open path end-to-end. Findings, highest-impact first:

1. **No list virtualization — the primary cause.** `FileList.svelte:291` is
   `{#each entries as entry, i (entry.path)}` rendering *every* entry, each with a per-row DOM ref
   (`bind:this={rowEls[i]}`), click/drag/context handlers, and tag sub-loops. A folder with a few
   thousand files builds a few thousand full rows up front. Explorer renders only the visible window;
   this app renders the whole tree. This alone typically costs seconds on large folders and is where the
   10× lives.
2. **O(n²) re-sort during streaming.** `loadPath` (`App.svelte:805`) does `entries = entries.concat(batch)`
   for each 256-row batch, and `$: visible = sortEntries(tagFiltered, …)` (`App.svelte:1081`) re-sorts the
   *entire* growing array on every batch — plus every downstream reactive derivation (`selectedEntries`,
   `itemCount`, `crumbs`, tag counts) recomputes each time. For an N-entry folder that's ~N/256 full
   sorts over a growing array. Coalesce batches, sort incrementally, and/or debounce the reactive
   recompute so the list settles once.
3. **Per-row DOM weight.** Even within the visible window, each row carries multiple handlers, bindings,
   and nested `{#each tagEntry.tags}` loops. Once virtualized this matters far less, but the row template
   is worth slimming for the ~30–50 rows actually on screen.
4. **Backend is probably NOT the bottleneck (verify, don't assume).** `stream_dir_entries`
   (`lib.rs:450`) uses `read_dir` + `entry.metadata()`; on Windows `DirEntry::metadata()` reuses the
   `FindNextFile` data with no extra syscall for non-reparse entries, and `is_hidden`/`extension_of` are
   cheap. Profile a large folder to confirm the wall-clock is in the webview, not Rust, before touching
   the backend. (If a folder is on a slow/network drive or full of reparse points, revisit.)

## Rough scope (NOT decomposed)
- **Measure first.** Add a repeatable benchmark: time-to-first-paint and time-to-settled for folders of
  ~100 / ~1k / ~10k / ~50k entries, split into backend walk time vs. frontend render time (console marks
  or a dev overlay). Every change is judged against these numbers — "10×" must be a measured before/after,
  not a vibe.
- **Virtualize the file list** — render only the visible window (+ overscan) in details, icons, and
  gallery views. Keep keyboard nav, selection, scroll-into-view, rename-in-place, and drag/drop working
  with virtualized rows (the `rowEls` ref array and `scrollIntoView` logic in `App.svelte` need to become
  window-aware). This is the headline item.
- **Fix the streaming recompute** — coalesce stream batches before re-sort, sort incrementally or once at
  settle, and prevent the O(n²) reactive cascade. Preserve final sort order and the streaming-liveness
  contract (STREAMING.md, CPE-662) — first rows still paint immediately.
- **Slim the row template** — reduce per-row handlers/bindings; hoist shared work out of the row.
- **Backend profiling pass** — confirm/deny the walk cost; only optimize Rust (e.g. parallel metadata,
  avoiding allocations in `dir_entry_from`) if the profile says so.
- **Guard against regressions** — a perf smoke test / budget so a future change can't silently reintroduce
  full-list rendering.

## Open questions (resolve at activation)
- **Virtualization approach:** hand-rolled windowing (zero dep, full control over selection/DnD/rename) vs.
  a small Svelte virtual-list library (faster to land, new dep, must not fight our interactions)?
- **Variable row heights:** icons/gallery tiles vs. details rows — fixed-height per view (simplest) or
  measured heights?
- **Batch coalescing vs. virtualization:** virtualization may make the O(n²) sort moot for paint, but sort
  itself is still O(n log n) per batch — do we still need incremental sort, or is one settle-sort enough?
- **10× baseline:** which folder size/drive defines "10×"? Pick the reference case up front so the target
  is falsifiable.

## Definition of Done (epic-level)
- A named reference folder that opens in ~Xs today opens in **≤ X/10s** to first interactive paint,
  measured before/after; large-folder scroll/sort/select stays at ~60fps.
- The file list renders only the visible window in all three views, with keyboard nav, selection,
  scroll-into-view, rename-in-place, and drag/drop intact.
- The streaming recompute no longer re-sorts the whole growing array per batch; streaming-liveness
  contract preserved.
- Backend walk time confirmed (profiled) as in- or out-of-budget; optimized only if the profile warrants.
- A perf budget/smoke test guards against regressing to full-list rendering.
- No regression on small folders; plain explorer stays fast/small/predictable.

## Notes
Right shape for the codebase: the win is a frontend rendering-architecture change (virtualization +
recompute discipline), not backend plumbing, which is already streamed and lean. `big-design` — the design
is making virtualization coexist with selection/DnD/rename/keyboard-nav, and defining a falsifiable 10×
benchmark. Builds on the streaming-liveness epic (CPE-662) rather than replacing it. Dormant brief until
activated with `/ticketing-epic activate CPE-688`.

## Work Log
2026-07-18 — Filed on user report ("Windows Explorer opens the same directory instantly; this app takes
multiple seconds — improve all file-explorer operations 10×"). Diagnosed the open path: backend is
streamed+lean; the cost is the un-virtualized file list (`FileList.svelte:291`) plus an O(n²) re-sort per
stream batch. Not decomposed; activate to plan.
