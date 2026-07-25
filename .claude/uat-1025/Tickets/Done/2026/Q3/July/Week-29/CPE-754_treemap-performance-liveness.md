---
id: CPE-754
title: Treemap performance & liveness — parallel walk, stale-while-revalidate, prefetch
type: bug
component: Multiple
tags: ready
created: 2026-07-19
closed: 2026-07-19
status: Done
priority: high
estimate: 3-4h
---

## Summary
GUI feedback: the disk-usage treemap's **Up / drill navigation is too slow** — it re-scans on every
path change. Researched how the fast tools do it (WizTree = NTFS MFT; **TreeSize = scan once, navigate a
cached tree, no rescan**; eDirStat = work-stealing parallel walk; stale-while-revalidate = instant + quiet
refresh) and applied the applicable patterns. Makes liveness first-order: **show now, refresh quietly.**

## What changed
### Backend — parallel walk (the raw-speed lever)
- `src-tauri/Cargo.toml`: `rayon` promoted to a direct dependency (already transitive).
- `src-tauri/src/lib.rs`: `dir_size_walk` now sums files inline and fans the recursive **sub-directory**
  walks across cores with `rayon` (work-stealing; symlink-skip preserved). `dir_children_sizes` reads the
  immediate children single-threaded, then computes each child's recursive size in parallel. Same results,
  much faster on multi-core — the eDirStat approach.

### Frontend — stale-while-revalidate + prefetch (the liveness lever)
- `src/lib/components/DiskSpaceView.svelte`:
  - **Navigate the cache, never re-walk** (TreeSize model): Up / re-drill of a visited folder is instant.
  - **Stale-while-revalidate**: a cached path paints instantly (no spinner), then re-scans in the
    background and swaps in fresh data *only if it changed* and we're still viewing it.
  - **Background prefetch**: after showing a level, the largest child folders are scanned into the cache
    (bounded to 8, deduped via an `inflight` set) so the likely next drill-in is instant.
  - A quiet `· refreshing…` indicator while background work runs; the blocking `· scanning…` only on a
    truly cold path.

### Install
- The reinstall now **clears the WebView2 HTTP/code cache** so the new frontend actually loads — a stale
  cached `index.html` was very likely why the CPE-753 cache fix looked ineffective ([[webview2-cache-survives-reinstall]]).

## Acceptance
- [ ] Up / re-drilling a visited folder is instant (served from cache, no re-walk).
- [ ] A cold scan is noticeably faster than before (parallel walk) on a large folder.
- [ ] Drilling into a large child that was prefetched is instant; the view refreshes quietly if data changed.
- [ ] `cargo test`/clippy green (both feature modes); `npm run check` clean.

## Notes / future
NTFS **MFT** reading (WizTree's 50× trick) is the next big lever but is admin-only + NTFS-only + Windows —
a future optimization (fits the instant-index epic CPE-703), not this ticket. On the CPE-753 branch so one
rebuilt sidecar carries the treemap + round-1 fixes + this.
