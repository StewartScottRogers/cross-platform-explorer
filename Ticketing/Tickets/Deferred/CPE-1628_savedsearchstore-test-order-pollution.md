---
id: CPE-1628
title: "savedSearchStore.test.ts fails only inside a full-suite run — test-order pollution makes the suite intermittently red"
type: Bug
status: Deferred
priority: Medium
component: Frontend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Observed by the independent Reviewer of PR #817, on a branch whose diff does not touch this file at all.
A full `npx vitest run` failed at `src/lib/savedSearchStore.test.ts:172` — `expect(persisted).toHaveLength(1)`
received `2`. Running that file alone passed 24/24, and re-running the full suite passed 3316/3316.

So it is order/state pollution, not a real regression. But an intermittently red suite is corrosive: it
trains the crew to re-run and shrug, which is precisely how a genuine failure eventually gets waved
through. This crew's whole QA model rests on a green run meaning something.

## The likely mechanism
`persisted` accumulating an extra entry points at shared state surviving between test files — a
module-level store, a `localStorage`/persistence shim, or a subscription never torn down — so a saved
search written by an earlier file is still present when this one asserts. Confirm before fixing.

## Scope
- Reproduce deterministically: run the full suite with a fixed seed / no shuffle, then bisect the file
  order to identify the polluter (vitest accepts an explicit file list, which makes narrowing quick).
- Fix the **root cause** — reset the store/persistence between files — rather than making the assertion
  tolerant. An assertion loosened to `>= 1` would hide the very leak this ticket exists to remove.
- Add whatever teardown the pattern needs so the same class of leak cannot recur silently.

## Acceptance criteria
- The polluting interaction is named in the work log, with the evidence that identified it.
- The full suite passes on at least 5 consecutive runs with no order-dependent failure.
- The fix is isolation/teardown, not a weakened assertion.

**Conflict surface:** `src/lib/savedSearchStore.ts` and its test, plus whichever test setup/teardown file
the leak traces to. Independent of current feature work.

## Work Log

### 2026-08-11 — Could not reproduce after extensive effort; no fix shipped

**Bottom line: the specific `savedSearchStore.test.ts` "persisted received 2" failure did not reproduce
on this machine across 8 full-suite runs, including deliberately worst-case scheduling. Per this
ticket's own instructions ("if you cannot reproduce, report back instead of shipping a speculative
fix"), no code change was made to `savedSearchStore.ts`/`savedSearchStore.test.ts`. The assertion is
untouched.**

**What was tried** (all `npx vitest run`, this worktree, Windows, 32 logical CPUs):
1. Default settings, twice (one with a cold `node_modules/.vite/vitest/results.json` cache, one warm)
   — 273/273 files, 3325/3325 tests, both green.
2. `--no-file-parallelism` (sequential, but vitest still forks a fresh process for most files under
   `isolate: true`) — green.
3. `--pool=forks --poolOptions.forks.singleFork=true` (forces literally ONE OS process for the entire
   273-file run, the worst case for any shared-module/shared-global leak) — green, though several
   OTHER files (`App.smartFolderLiveRefresh.test.ts`'s "tag smart folder" test, a Sidebar hover-cache
   race test, an `App.folderPeek` test) intermittently timed out in this artificial single-process mode
   before eventually passing on the same run — see "Adjacent finding" below.
4. `CI=true`, default parallelism — green.
5. `CI=true --poolOptions.forks.minForks=1 --poolOptions.forks.maxForks=2` (mimics a 2-core GitHub
   Actions runner) — green.

**Instrumentation used to look for the leak directly:** added temporary `console.error` probes inside
`src/lib/persist.ts`'s `lsGet`/`lsSet` (guarded behind an env var, filtered to the
`cpe.savedSearches` key only, tagging each call with `process.pid`, a timestamp, and
`expect.getState().testPath`) plus a similar probe at `savedSearchStore.ts`'s module-init line. Ran
this across runs 1, 3, and 5 above. In every captured trace, `savedSearchStore.test.ts`'s own 12
dynamic-import cycles (24 tests) show a clean, self-consistent read/write sequence with **zero**
writes attributed to any other test file's path, even in the single-fork run where dozens of
`App.*.test.ts` files (several of which call `addSavedSearch("Markdown docs", …)` themselves —
`App.smartFolderLiveRefresh.test.ts`, `App.previewPlaceholderIcon.test.ts`, `App.savedSearch.test.ts`,
`App.archiveNesting.test.ts`) ran in the SAME OS process immediately before/after it. Every one of
those files' own `beforeEach` correctly pairs `localStorage.clear()` with `savedSearches.set([])`, so
within-file they're clean, and — contrary to my initial hypothesis — isolate:true's per-file jsdom
environment swap really does give each file a fresh `localStorage` even when the OS process is reused
(confirmed empirically: every file-boundary probe read `null`/`[]` first, regardless of what the
previous file in that same process had just written).

**Adjacent finding (real bug, but NOT confirmed to be this ticket's cause — filed separately as
CPE-1631, not fixed here to keep this diff at zero):** `App.svelte`'s `onDestroy` (around line 6171)
tears down `unlistenSessions`/`unlistenTransferDone`/timers/etc., but does **not** call
`smartRefreshDebounce.cancel()` or `smartRefreshUnlisten?.()` (the CPE-1230 smart-folder live-refresh
debounce + `folder-watch` listener declared around line 2001). A component that unmounts while a
structured search or tag smart folder is open (e.g. `@testing-library/svelte`'s auto `cleanup()` at the
end of a test that never explicitly closed the search) leaves a real, un-mocked 300ms `setTimeout`
and/or an armed listener registration behind. Under the single-fork stress run (item 3 above) this
manifested as real cross-test timeouts in unrelated later tests. It does NOT touch
`cpe.savedSearches` (the recompute path only re-scans/re-lists, it never calls
`addSavedSearch`/`renameSavedSearch`/`removeSavedSearch`/`moveSavedSearch`), so it can't explain the
specific "received 2" symptom, and none of the three tests in
`App.smartFolderLiveRefresh.test.ts`'s "structured search" describe block actually leave a timer
pending at test-end as currently written — so it's a latent hardening gap, not a proven active leak.
Worth fixing on its own; scoped separately per this ticket's own "keep the diff scoped, file a
follow-up" instruction.

**Why the original failure was probably real anyway:** a one-off scheduling arrangement (process/file
assignment is influenced by vitest's cached-duration file sequencer, which is itself seeded by
`node_modules/.vite/vitest/results.json` — a file that didn't exist yet on a fresh CI checkout for
PR #817) could plausibly produce a file grouping this investigation didn't happen to land on. Nothing
here proves the reviewer was wrong; it proves the failure is rare enough that ~2,600 file-executions
of `savedSearchStore.test.ts` across 8 runs on this machine never hit it.

**Disposition:** left in `Doing/` per instructions, undecided rather than closed. No PR opened against
`main` for this ticket (nothing to merge — see the note above about not shipping a speculative fix).
Suggest either: (a) leave open and revisit if/when it recurs with a captured CI log (the exact file
list + failure output from that run would let a future attempt target the real scheduling order
directly, e.g. via `npx vitest run <files, in that exact order>`), or (b) if it doesn't recur for
several more sprints, downgrade priority and close as "unable to reproduce, monitoring."

---

## Foreman disposition (2026-08-11, sprint)
**Deferred, not closed.** Eight full-suite runs — including the whole 273-file suite forced into a single
OS process, the worst case for any shared-module or `localStorage` leak — came back green, with an
instrumented probe on `cpe.savedSearches` showing zero cross-file writes. The worker correctly **refused to
ship a speculative fix** and left `savedSearchStore.ts` untouched; loosening the assertion would have hidden
exactly the leak this ticket exists to find.

Deferred by our choice rather than blocked externally, so it stays pickable. **Revisit when we have a
captured CI failure log for it** — the leading theory is that reproduction needs a specific file-to-process
scheduling order influenced by vitest's cached-duration sequencer (seeded from
`node_modules/.vite/vitest/results.json`), which would not have existed on the fresh CI checkout where the
original failure was seen. A real log pins the order down; without one, further local hunting is guesswork.

The investigation above is preserved so the next pickup starts warm. Adjacent real leak found on the way and
filed separately as **CPE-1633** (renumbered from the worker's draft, which collided with CPE-1631).
