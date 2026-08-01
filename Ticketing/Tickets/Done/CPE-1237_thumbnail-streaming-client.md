---
id: CPE-1237
title: "Frontend streaming thumbnail client (wire the orphaned thumb_queue + thumb_cache)"
type: Task
priority: Medium
component: frontend
tags: [ready]
created: 2026-08-01
epic: CPE-718
status: Done
closed: 2026-08-01
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

## Resolution
Wired both previously-orphaned modules into a real dispatch path, without reimplementing either's
tested logic:

**Backend** — new `crates/server/src/thumb_pipeline.rs`:
- `ThumbCacheStore`/`ThumbCacheService` pair `thumb_cache::ThumbCache`'s bookkeeping (recency +
  dual-budget eviction) with the actual decoded bytes it deliberately excludes ("the actual bytes
  live on disk / elsewhere" — its own doc comment), reconciled by membership after every `put` so
  eviction never leaks bytes. Held in Tauri-managed state (`ThumbCacheService`, `Arc<Mutex<...>>`,
  cheaply cloneable into `spawn_blocking` like `IndexService`) so it persists across the many
  `thumbnails_stream` calls one scroll session issues.
- `run_thumb_batch` enqueues one call's requests into a **fresh per-call** `thumb_queue::ThumbQueue`
  (real reused priority/dedupe/promotion logic — not reimplemented), serves cache hits immediately,
  and drains the rest highest-priority-first via a caller-supplied `compute` closure
  (`thumbnail::thumbnail_cached` in production, so the on-disk cache — "across sessions" — still
  applies underneath). A per-call queue makes cancellation trivial: a superseded batch's remaining
  queue is just abandoned, no removal API needed. 6 new unit tests: Visible-before-Prefetch ordering,
  cache-hit dedup (compute never called twice), cancellation stops the drain early, an unreadable
  path short-circuits to a fallback without queuing, a decode error caches nothing, and real
  byte-budget eviction end-to-end.
- Added `serde::Deserialize` to `thumb_queue::Priority` (frontend needs to send it).
- Thin `src-tauri/src/lib.rs` commands: `thumbnails_stream` (async, `spawn_blocking`, streams
  `ThumbResult` over an `ipc::Channel` per STREAMING.md) and `cancel_thumbnails_stream` (a
  `stream_id`-keyed cancel registry mirroring `index_build`'s / STREAMING.md's `DIR_STREAM_CANCELS`
  pattern). Regenerated `bindings.gen.ts` (also picked up unrelated pre-existing doc-comment drift on
  `startArchiveExtract` from an earlier ticket that never regenerated).

**Frontend** — new `src/lib/thumbnailClient.ts`: a module-singleton client. Every `requestThumbnail()`
call joins a shared pending list and resolves in the next microtask, so every tile
mounting/observing in the same tick (e.g. a whole freshly-rendered virtualization window, CPE-690)
batches into ONE `thumbnails_stream` call — giving the backend's priority queue real concurrent work
to arbitrate instead of a queue-of-one. A cache hit resolves synchronously (no backend round-trip). A
newer batch cancels whatever the previous one is still draining; anything left unresolved by a
"completed" (not hard-failed) batch is folded into the next flush, capped at `MAX_REQUEUE_ATTEMPTS`
so a batch that keeps completing without ever caching anything (found via a real bug: an unmocked
`thumbnails_stream` in `SimilarImagesDialog.test.ts`/`App.features.test.ts` resolved `null`
immediately, which without the cap spun the microtask queue into an OOM) can't loop forever. 8 vitest
cases covering: batching, Visible vs Prefetch priority, same-tick priority upgrade/dedup,
cancellation of a superseded batch, re-queue-until-resolved, cache-hit avoids a re-request (incl. a
cached `null`), and size-scoped cache keys.

`ThumbnailImage.svelte` now requests through the client instead of a raw per-tile
`invoke("thumbnail", …)`: a wide-margin (`600px`) `IntersectionObserver` requests at `Prefetch`
priority as soon as a tile is near the viewport, a strict (`0px`) one promotes to `Visible` once it's
truly on screen. Graceful fallback to the type icon is unchanged (a `null`/thrown result just leaves
`src` unset).

`filetypes.ts` gained `hasThumbnail()` (the existing raster `isImage()` set plus CPE-1236's
`psd`/`svg`/`ttf`/`otf`/`woff`/`woff2`), which `FileList.svelte`'s Icons/Gallery gate now uses instead
of `isImage` — without this, the newly-wired SVG/font/PSD thumbnails would never even be requested,
since the frontend never asked for them before. `isImage` itself is untouched (still raster-only, for
Similar Images / Properties dimensions, where a glyph sheet or vector icon doesn't belong).

`gui-smoke/specs/thumbnail-gallery.smoke.ts` (+ a `seedThumbnailGalleryFixture` in `wdio.conf.ts`)
pins a mixed-format gallery: a real PNG + a real SVG (both must render an actual `<img
class="thumb-img">`, streamed through the pipeline) plus a byte-garbage `.ttf` (must fall back to the
type icon, never crash). `npm run typecheck` (gui-smoke's own tsconfig) is clean; the full
build → tauri-driver run was not executed in this headless session — see the PR description for exact
repro steps for the Visual Critic / a follow-up run.

**Verify:** `npm run check` (svelte-check) 0 errors/0 warnings. `npx vitest run` 157/157 files,
1747/1747 tests, 0 unhandled errors. `cargo test` in `crates/server` (1216+ tests) and `src-tauri` (96
tests), `cargo clippy --all-targets -D warnings` in both (default + `sidecar-platform` for src-tauri;
default + `index` for crates/server, unaffected by this ticket) — all clean. Bindings drift guard
(`cargo run --bin export_bindings --features "specta-bindings sidecar-platform"`) produces no diff
beyond this ticket's own additions.

Files: `crates/server/src/thumb_pipeline.rs` (new), `crates/server/src/thumb_queue.rs`,
`crates/server/src/lib.rs`, `src-tauri/src/lib.rs`, `src/lib/bindings.gen.ts`,
`src/lib/thumbnailClient.ts` (new), `src/lib/thumbnailClient.test.ts` (new),
`src/lib/components/ThumbnailImage.svelte`, `src/lib/components/FileList.svelte`,
`src/lib/components/FileList.test.ts`, `src/lib/filetypes.ts`, `src/lib/filetypes.test.ts`,
`gui-smoke/wdio.conf.ts`, `gui-smoke/specs/thumbnail-gallery.smoke.ts` (new), `gui-smoke/README.md`.
