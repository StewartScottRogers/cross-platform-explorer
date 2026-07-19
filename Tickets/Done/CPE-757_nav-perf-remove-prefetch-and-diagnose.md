---
id: CPE-757
title: Explorer nav perf — remove harmful prefetch + on-screen timing diagnostic
type: bug
component: Frontend
tags: ready
created: 2026-07-19
closed: 2026-07-19
status: Done
priority: high
estimate: 1h
---

## Summary
Reported: ~6 s to display **20 files** on open, and the same on Back / sibling revisit. That's not a
big-folder render problem (20 rows) and it's slow even when the listing is cached — so the cost is
elsewhere. Measured `git status` on the repo: **0.02 s** — so `forge_repo_status` is not it.

Prime suspect: the **subfolder prefetch** I added in CPE-756 fired ~13 concurrent `list_dir` calls (a
*synchronous* backend command) on every navigation; the prefetch from the previous folder can block the
next folder's listing → persistent multi-second stalls, matching the symptom exactly.

## Changes
- **Removed the aggressive subfolder prefetch** (CPE-756's `prefetchDirs`/`prefetchOne`). The listing
  cache (instant Up/Back/re-open) stays.
- Background **revalidate now uses `rawInvoke`** (no busy cursor) and is **deferred 300 ms** so it can
  never make the just-painted navigation feel blocked.
- Added a **temporary on-screen perf readout** (bottom-left): last navigation's
  `paint / settle · N items · git ms · disk ms` — release builds have no devtools, so this lets us *see*
  where the time actually goes and stop guessing.

## Acceptance
- [ ] Up / Back / re-open is instant (cache), with no per-nav prefetch pile-up.
- [ ] The perf readout shows the real breakdown so we can pinpoint any remaining cost (list vs git vs disk).
- [ ] `npm run check` clean; App suites pass.

## Notes
Diagnostic build — the on-screen readout + any confirmed root-cause fix will be cleaned up once the
slowness is understood. If the readout shows `settle` itself is multi-second, the backend fs commands are
the block and the fix is to make them async (`spawn_blocking`) so they don't stall the main thread.
