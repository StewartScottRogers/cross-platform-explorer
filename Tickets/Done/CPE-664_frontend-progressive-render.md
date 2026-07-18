---
id: CPE-664
title: Frontend progressive middle-pane render from the stream
type: feature
component: Frontend
priority: high
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-662
estimate: 2-3h
---

## Summary
Child of CPE-662. Replace the blocking `entries = await invoke("list_dir")` at `App.svelte:743` with a
`Channel` subscription: reset `entries = []`, append each incoming batch (the reactive `visible` pipeline
re-sorts automatically), and clear loading when the stream ends. Add a **generation token** so navigating
away mid-load supersedes the in-flight stream (stale batches dropped, no bleed into the new folder), and a
lightweight loading affordance replacing the busy-cursor-until-done model. Prereq: CPE-663.

## Acceptance Criteria
- [x] `loadPath` streams via `list_dir_stream`, painting the first batch immediately.
- [x] Sort/filter/tag pipeline still produces the same final ordering as the one-shot path.
- [x] A folder change mid-load supersedes the previous stream (generation guard); no stale rows appear.
- [x] Small folders are no slower than before (spot-measured); error + empty-folder + HOME paths intact.
- [x] `npm run check` clean; targeted jsdom/unit coverage for the supersede guard where feasible.

## Work Log

2026-07-18 (nightshift) — Picked up (prereq CPE-663 landed). Estimate 2-3h.

## Resolution
`loadPath` (src/App.svelte) now opens a `Channel<DirEntry[]>` and calls `list_dir_stream` via `rawInvoke`
(self-progress opt-out of the busy cursor). Each batch appends to `entries` and flips `loading=false` so
the first rows reveal immediately (FileList gates on `!loading`); the existing reactive `visible` pipeline
re-sorts as entries grow, so final order is unchanged. A monotonic `loadGen` token supersedes an in-flight
stream on navigation — stale batches are dropped and the post-load hooks (recent-folder, selection remap,
pending rename/select) only run for the winning generation. Updated the two App integration-test mocks
with a Channel stand-in + list_dir_stream case; check clean, full suite green (653). Docs note added
(03-explorer.md). Files: src/App.svelte, src/App.test.ts, src/App.features.test.ts, src/docs/03-explorer.md.

Live perceptual verification (huge-folder feel) is best confirmed on a real build via /run.
