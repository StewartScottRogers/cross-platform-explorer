---
id: CPE-762
title: Simplify DiskSpaceView — drop harmful prefetch, reactive derived state, reuse baseName
type: chore
component: Frontend
tags: ready
created: 2026-07-19
closed: 2026-07-19
status: Done
priority: medium
estimate: 1h
---

## Summary
Code-quality cleanup of `src/lib/components/DiskSpaceView.svelte` (the disk-usage "Space" treemap,
CPE-751), surfaced by a `/simplify` review pass (reuse / simplification / efficiency / altitude). The
component had accreted stale-while-revalidate machinery — a per-path cache **plus** background
`revalidate`, an 8-wide `prefetch`, an `inflight` Set, and a `refreshing` flag — that three of the four
reviewers independently flagged as both over-complex and inefficient. The eager `prefetch` (up to 8
concurrent recursive `dir_children_sizes` walks per navigation, each internally rayon-parallel) is the
**exact anti-pattern CPE-757 already deleted** from the sibling `App.svelte` listing cache ("its many
concurrent calls piled up and stalled the next navigation").

## Scope
- Remove `prefetch`, `revalidate`, `inflight`, `refreshing`, `sameChildren`, and `applyChildren`.
- Keep the simple per-path `cache` (cold scan once, instant Up / re-drill — the real win) — scanning
  happens once per path while the modal is open.
- Derive `total` / `byKey` / `tiles` reactively from `children` (`$:`) instead of imperatively
  recomputing all three by hand in `applyChildren`.
- Reuse the shared `baseName` helper from `contentSearch.ts` instead of the bespoke `baseOf` regex
  (a fourth path-basename implementation that could silently drift from every other view).

## Correctness fix discovered during cleanup
While rewriting the initialization area, found that **the modal never scanned on open**: `onMount` was
imported but never called, and `scan()` was only reachable from `drill` / `up`. With `loading`
initialized `true` and nothing to flip it, opening the Space view would sit "· scanning…" forever over a
blank treemap. Wired up the intended `onMount(() => scan(cur))` (the dangling `onMount` import was the
vestige of the dropped call). This was out of the strict `/simplify` remit (correctness, not cleanup)
but is a one-line fix in the exact code being rewritten, so it was folded in rather than left broken.

## Explicitly NOT changed (reviewed, judged right as-is)
- The ~62 `spawn_blocking(move || X_impl(args)).await` command wrappers in `lib.rs` — the altitude
  reviewer blessed the explicit form (varied result tails, `#[cfg]` attributes, Tauri needs the visible
  typed signature, and the `_impl` split is required for runtime-free unit tests). A macro would be a
  lateral move that hurts grep-ability / stack traces.
- `treemap.ts` squarify inner loop is O(n²) over a row — real but micro (n = visible children only);
  rewriting it risks changing layout on a verified module for negligible gain.
- Diagnostics timing at the single `invoke` chokepoint and the `App.svelte` `dirCache` are at the right
  altitude (confirmed zero-cost when off; view-layer cache belongs in the view).

## Acceptance
- [x] `prefetch` / `revalidate` / `inflight` / `refreshing` machinery removed
- [x] `total` / `byKey` / `tiles` derived reactively from `children`
- [x] `baseOf` replaced with shared `baseName` from `contentSearch.ts`
- [x] Space view scans on open (initial `onMount` scan wired)
- [x] `npm run check` clean (0 errors / 0 warnings)
- [x] `npx vitest run` green (740 tests)

## Resolution (closed 2026-07-19)
Rewrote the `DiskSpaceView.svelte` script per the four-agent `/simplify` review: dropped the
prefetch/revalidate/inflight/refreshing stale-while-revalidate machinery (net simpler and removes the
CPE-757 concurrency anti-pattern), made the three treemap-derived values reactive off `children`,
reused the shared `baseName` helper, and wired the missing `onMount` initial scan. Net **−~70 lines**
in one file, no backend change. Verified: svelte-check 0/0, 740 frontend tests pass. Landed on `main`.

## Work Log
- 2026-07-19 — Picked up from a `/simplify` pass on the merged perf/diagnostics chain (b83ac61..d098597).
  Estimate: 1h. Four review agents converged on DiskSpaceView as the one file needing cleanup.
- 2026-07-19 — Applied edits; discovered + fixed the missing initial-scan wiring. `npm run check` 0/0,
  `npx vitest run` 740 passed. Closed and merged to main.
