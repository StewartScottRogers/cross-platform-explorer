---
id: CPE-756
title: Explorer navigation — directory listing cache (instant Up/Back) + SWR + prefetch
type: bug
component: Frontend
tags: ready
created: 2026-07-19
closed: 2026-07-19
status: Done
priority: high
estimate: 2-3h
---

## Summary
GUI feedback: browsing up/down the folder tree is still too slow. Diagnosis: every navigation re-lists the
folder from scratch (`loadPath` → `list_dir_stream`), with **no listing cache** — so Up / Back / re-opening
a folder you were just in re-scans and re-renders it. (Separately, the file list is not virtualized on this
build — that's the desktop session's CPE-690 render work, deliberately left untouched here.)

Applied the same liveness pattern that fixed the treemap (CPE-754): the TreeSize/stale-while-revalidate
model — **navigate a cached listing, revalidate quietly, prefetch neighbours.**

## What changed (`src/App.svelte`)
- A bounded LRU **`dirCache`** (path → entries, 48 entries).
- `loadPath` gains a `useCache` flag. **Navigations** (`navigate`, `goBack`, `goForward`, and so `goUp`)
  pass it: a cached folder **paints instantly** (no re-stream), then:
  - **Stale-while-revalidate** — re-lists in the background (plain `list_dir`) and swaps the rows in only
    if the listing actually changed and it's still the active view.
  - **Prefetch** — warms the parent (for Up) and the top ~12 subfolders (for drill-down) into the cache.
- A **cold** load streams as before and now **caches the settled listing**.
- **Reloads after a mutation** (refresh, file ops — all the direct `loadPath(currentPath)` calls) stay
  `useCache=false`, so our own changes are always shown fresh and the cache is refreshed.

## Acceptance
- [x] Up / Back / re-opening a recently-visited folder is served from the cache (no re-stream) — instant.
- [x] Drilling into a prefetched subfolder is served from cache (prefetch warms the top ~12 subfolders).
- [x] A cache-served folder revalidates in the background (`list_dir`) and swaps in only if it changed.
- [x] Reloads after a mutation (refresh + all `loadPath(currentPath)` file-op calls) stay `useCache=false`,
  so our own create/delete/rename always shows fresh content and refreshes the cache.
- [x] `npm run check` clean; App suites pass (updated the CPE-676 back-navigation test to assert the
  navigation *outcome* via the `aria-current` breadcrumb, since navigation no longer necessarily re-streams).

## Notes / boundary
Latency fix only — orthogonal to list **rendering**. Big folders still render every row until virtualization
(CPE-690, desktop session) lands; that's the complementary fix and is intentionally not touched here to avoid
clobbering that in-progress work. On the CPE-754 branch so one build carries treemap + perf + this.
