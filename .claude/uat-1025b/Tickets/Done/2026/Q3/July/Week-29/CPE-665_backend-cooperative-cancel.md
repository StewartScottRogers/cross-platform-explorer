---
id: CPE-665
title: Cooperative cancellation for superseded dir streams
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-662
estimate: 1-2h
---

## Summary
Child of CPE-662. Today a superseded stream (user navigated away mid-load) is only dropped frontend-side;
the backend keeps walking a huge folder to completion invisibly. Add cooperative cancellation: a
generation/`AtomicBool` registry (or a cancel command keyed by a stream id) the walker checks between
batches so a superseded `list_dir_stream` stops promptly. Prereq: CPE-663/664.

## Acceptance Criteria
- [x] A superseded stream stops walking within a batch or two rather than finishing the whole directory.
- [x] No leaked registry entries (cleaned up on completion and cancellation); thread-safe.
- [x] cargo-tested cancellation path; clippy clean both feature modes.

## Work Log
2026-07-18 (nightshift) — Picked up (prereqs 663/664 landed). Mirroring the transfer cancel registry. Estimate 1-2h.

## Resolution
Mirrored the transfer cancel-registry idiom (src-tauri/src/lib.rs): a `DIR_STREAM_CANCELS`
`OnceLock<Mutex<HashMap<u64, Arc<AtomicBool>>>>` + `dir_stream_registry()` accessor. `stream_dir_entries`'
flush closure now returns `ControlFlow` so a caller can stop the walk at a batch boundary; `list_dir`
always continues. `list_dir_stream` gained a frontend-supplied `stream_id`, registers a cancel flag it
polls each batch, and removes it on completion; new `cancel_dir_stream(stream_id)` sets the flag (no-op
for a finished id). Frontend `loadPath` passes `streamId: gen` and fires `cancel_dir_stream(gen-1)` when a
new navigation supersedes the previous stream, so a huge/slow folder stops churning after you move on.
3 cargo tests (break stops walk, cancel sets flag, unknown-id no-op); 120 backend tests pass; clippy clean
both feature modes; check + suite green. Files: src-tauri/src/lib.rs, src/App.svelte.
