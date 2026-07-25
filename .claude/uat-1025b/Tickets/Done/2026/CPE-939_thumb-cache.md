---
id: CPE-939
title: "Thumbnail cache/key/eviction core"
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-718
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary

First headless slice of epic CPE-718 (Universal thumbnail pipeline): the pure, std-only
cache-management core the pipeline sits on, in the Tauri-free `cpe-server` crate as
`crates/server/src/thumb_cache.rs`. This is *not* image/video decoding — it is the key + cache +
eviction + coalescing model the per-format extractors plug into.

Provides:
- `thumb_key(path, mtime_ms, size_bytes, target_px) -> String` and a `ThumbKey` struct — a stable,
  collision-resistant, deterministic key (dual `DefaultHasher` fold, 32 hex chars) so an edited file
  (mtime/size change) or a different tile size is a cache miss.
- `ThumbCache` — an LRU cache (`HashMap` + recency `VecDeque`) bounded by *both* a max entry count and
  a max total byte budget; `put` evicts least-recently-used until within both budgets, `get` promotes
  to most-recently-used.
- Request coalescing — `begin(key) -> bool` (false if already in-flight) / `finish(key)`, backed by a
  `HashSet`, so the same missing thumbnail isn't computed twice concurrently.

std-only, no new dependencies. `serde::Serialize` derived on the public types.

## Acceptance Criteria

- [x] New `crates/server/src/thumb_cache.rs`, std-only, no new deps.
- [x] `thumb_key` + `ThumbKey` — deterministic and sensitive to path/mtime/size/target_px.
- [x] `ThumbCache` LRU bounded by max entry count AND max total bytes; `put` evicts LRU, `get` promotes.
- [x] Coalescing: `begin` returns false while a key is in-flight, `finish` clears it.
- [x] `Debug`/`Clone`/`serde::Serialize` derived where useful.
- [x] Thorough `#[cfg(test)]` tests: key stability + sensitivity, LRU eviction by count and by bytes,
  recency promotion, coalescing. (14 new tests, all passing.)
- [x] Module registered as `pub mod thumb_cache;` in `crates/server/src/lib.rs`.
- [x] `cargo test thumb_cache` passes; `cargo clippy --all-targets -- -D warnings` clean.

## Work Log

- 2026-07-23: Wrote `thumb_cache.rs` (ThumbKey/thumb_key, ThumbCache with dual-budget LRU + coalescing),
  registered the module in `lib.rs`, added 14 unit tests. `cargo test thumb_cache` → 14/14 new tests
  pass; `cargo clippy --all-targets -- -D warnings` clean. Activated epic CPE-718. Branch
  `cpe-939-thumb-cache`, PR opened (do-not-merge; orchestrator serializes merges).
