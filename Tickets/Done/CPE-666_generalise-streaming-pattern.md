---
id: CPE-666
title: Generalise + document the streaming-liveness pattern
type: task
component: Multiple
priority: medium
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-662
estimate: 2-3h
---

## Summary
Final child of CPE-662 (its DoD "applied to at least one other producer + documented"). Apply the
channel-streaming shape to one more large/slow producer beyond `list_dir` (e.g. `read_archive_entries` or
aligning the search results path), and write up the house style — when to stream vs. return a vec, the
`ipc::Channel` batch convention, and the generation-token supersede — as a design doc under
`docs/design/`. Prereq: CPE-664.

## Acceptance Criteria
- [x] A second bulk producer streams via `ipc::Channel` with progressive frontend render.
- [x] `docs/design/STREAMING.md` (or similar) documents the pattern; in-app docs updated if user-facing.
- [x] `npm run check` clean; suite green.

## Work Log
2026-07-18 (nightshift) — Picked up (prereq 664 landed). Second producer = find_files_by_name (Ctrl+P). Estimate 2-3h.

## Resolution
Second streaming producer: filename search (Ctrl+P). Extracted the tree walk into a shared
`walk_name_matches(root, query, batch, flush)` (ControlFlow-based, same caps/skip behaviour);
`find_files_by_name` collects through it, new `find_files_by_name_stream(root, query, on_match:
ipc::Channel<Vec<NameMatch>>)` streams hit-batches (32) and returns the final dirs_scanned/truncated.
`FileNameSearchDialog` now opens a Channel, appends hits progressively (flips loading off on the first
batch; reactive `hits` re-sorts), with a generation-token supersede. Documented the whole convention in
docs/design/STREAMING.md and added a CLAUDE.md pointer; in-app docs note that search results stream.
Backend parity test + updated App.features mock; check + suite green (653); clippy clean both modes.
Files: src-tauri/src/lib.rs, src/lib/components/FileNameSearchDialog.svelte, src/App.features.test.ts,
docs/design/STREAMING.md, CLAUDE.md, src/docs/03-explorer.md.
