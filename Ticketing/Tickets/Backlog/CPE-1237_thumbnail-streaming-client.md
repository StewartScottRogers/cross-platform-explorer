---
id: CPE-1237
title: "Frontend streaming thumbnail client (wire the orphaned thumb_queue + thumb_cache)"
type: Task
priority: Medium
component: frontend
tags: [ready]
created: 2026-08-01
epic: CPE-718
closed:
prereq: CPE-1236
---

## Context
`thumb_queue.rs` (priority scheduler Visible>Prefetch>Background, CPE-950) and `thumb_cache.rs`
(dual-budget LRU, CPE-939) are BUILT + cargo-tested but wired into NO command/dispatch;
`ThumbnailImage.svelte` bypasses both with a naive per-tile eager decode. Wire them in so thumbnails
for visible virtualized tiles are requested through the priority queue + served from the cache, streamed
in, off the UI thread.

## Acceptance criteria
- Visible icon/gallery tiles (respect CPE-690 virtualization — only visible + a prefetch margin) request
  thumbnails through the priority queue; results stream in + cache across sessions.
- Scrolling stays smooth (no eager decode of off-screen tiles); off-screen requests are lower priority /
  cancellable.
- Graceful fallback to the type icon when a format can't be rendered.
- Feature-off (or no thumbnails) incurs no cost — plain views unaffected.
- Reuse STREAMING.md / the existing `ipc::Channel` + busy-cursor conventions; `invoke` via
  `src/lib/invoke.ts`. If a new command is needed to drive the queue, thin dispatcher in lib.rs +
  domain logic in cpe-server; regen bindings if a specta struct changes.
- REAL tests: vitest for the client (queue ordering / cancellation / cache hit) + a gui-smoke render pin
  of a mixed-format gallery showing streamed thumbnails.

## Notes
Prereq CPE-1236 (extractors) so the client has real per-format thumbnails to stream. Monetizes
already-built orphaned engines (the CPE-978 pattern).
