---
id: CPE-1840
title: the status bar's two count fields have no staleness coverage, so a stale count can ship unnoticed
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

The status bar carries two counts derived from a listing — `filteredHidden` ("N hidden by filter",
pre-existing) and `unreadableCount` ("N entries could not be read", added by CPE-1780). Both have a
**first-paint** path that is genuinely pinned, and **three staleness paths that are pinned nowhere**.

All three mutations below are line-count-preserving and leave the whole suite green at 319 files / 4223
tests, measured during the CPE-1780 review:

1. **Delete the cache-served reset** (`ExplorerPane.svelte`, cache branch: `unreadableCount = 0`). A cache
   hit then carries the *previous folder's* count on screen until revalidation finishes — contradicting
   the prop's own documented contract.
2. **Weaken the `<StatusBar>` staleness gate** from
   `isHome || archive || smartFolder || structuredSearch ? 0 : unreadableCount` to `isHome ? 0 :
   unreadableCount`. Only the `isHome` arm is tested (via Ctrl+T); the archive, smart-folder and
   structured-search arms are unpinned, so the count can survive into a view it does not describe.
3. **Delete `unreadableCount = fresh.unreadable`** in `revalidateDir`. The count then never updates on
   revalidation.

`filteredHidden` has **identical holes in all three places**. That is why this is one ticket rather than
two: the mutations, the fix and the tests are the same shape for both fields, and splitting them would
duplicate the work.

## Why it matters

These counts exist to stop the app making a false statement about a folder — CPE-1708 and CPE-1780 are
both about a listing quietly shorter than the folder really is. A count that is *correct on first paint
and stale thereafter* makes exactly that false statement, in a way nobody would notice, because the
number looks plausible.

It is not a regression: CPE-1780 shipped `unreadableCount` at exact parity with the already-merged
`filteredHidden`, which is why it was not a merge blocker. It is a gap both fields share.

## Acceptance criteria

- [ ] Each of the three staleness paths is pinned for **both** fields — cache-served reset, the
      `<StatusBar>` gate's non-`isHome` arms, and the `revalidateDir` update.
- [ ] Red-proof every new test with the exact mutation above: make the one-line change, observe red,
      revert, record which line. All three currently leave the suite green, so a test that does not red
      under them has not closed the gap.
- [ ] The `<StatusBar>` gate is pinned per arm, not as a whole — a single test that only exercises
      `isHome` is what left three arms uncovered in the first place.
- [ ] Check whether any other listing-derived prop threaded to the status bar has the same shape, and say
      so either way rather than fixing only the two named here.

## Notes

Filed from the CPE-1780 review, where the Reviewer swept for line-count-preserving mutations after the
split and found these three. It explicitly recommended one ticket covering both fields, and judged none
of them blocking because the first-paint path is pinned and the new field ships at parity with the
existing one.

Related: CPE-1833 (those same two notes are never announced to a screen reader), CPE-1836 (the row's
layout at the 600px floor), CPE-1838 (the in-flight-listing mechanism these counts ride on).

## Work Log

### 2026-08-21 — tests only, no production change

Added `src/App.statusBarCountStaleness.test.ts` (8 tests, App-level integration against the mocked-Tauri
harness). All three staleness paths are now pinned for BOTH fields:

- **cache-served reset** (2 tests) — Home -> `C:\d` -> `C:\d\photos` (counted) -> Back to `C:\d` (a cache
  hit, `goBack` passes `useCache=true`). `C:\d`'s `list_dir` is HELD (never resolves), so "the note is
  gone" can only be satisfied by the cache branch's own reset, never by a revalidation racing in behind it.
- **`revalidateDir` update** (2 tests) — the stream's count and `list_dir`'s count are deliberately
  different (0 cached, 4/6 fresh), so a refreshed count is distinguishable from a remembered one.
- **`<StatusBar>` gate, per arm** (4 tests: Home, archive, smart folder, structured search) — each entered
  from a folder carrying 2 filtered + 3 unreadable, and none of `enterArchive`/`openSmartFolder`/
  `openStructuredSearch` calls `loadPath`, so the counts are still live in App state and the gate is the
  only thing that can clear them. Each arm asserts BOTH notes.

Red-proof (every mutation applied, suite run, reverted; `git status` clean after each):

| Mutation | Reds |
|---|---|
| delete `ExplorerPane.svelte:379` (`filteredHidden = 0`, cache branch) | cache-served reset / filteredHidden (1 failed, 7 passed) |
| delete `ExplorerPane.svelte:380` (`unreadableCount = 0`, cache branch) | cache-served reset / unreadableCount |
| delete `ExplorerPane.svelte:341` (`filteredHidden = fresh.filtered`) | revalidation / filteredHidden |
| delete `ExplorerPane.svelte:342` (`unreadableCount = fresh.unreadable`) | revalidation / unreadableCount |
| `App.svelte:6917` gate -> `isHome ? 0 : filteredHidden` | archive + smart folder + structured search (3 failed) |
| `App.svelte:6918` gate -> `isHome ? 0 : unreadableCount` | archive + smart folder + structured search (3 failed) |
| `App.svelte:6917` gate -> drop the `isHome` arm | Home arm |
| `App.svelte:6918` gate -> drop the `isHome` arm | Home arm |

Gates: `npx vitest run` 321 files / 4266 tests passed; `npm run check` 0 errors, 0 warnings.

**Enumeration of the other listing-derived status-bar props** (the AC's fourth box):

- `itemCount` / `totalCount` — genuinely listing-derived and they carry their own view gate
  (`(isHome && !smartFolder && !structuredSearch)`, plus `|| archive` for `totalCount`), but they are
  **structurally immune** to all three staleness paths: they derive from `visible`/`shown`, which
  `<ExplorerPane>` recomputes from `entries` / `smartOverride` / `archiveOverride`. The cache branch
  replaces `entries` in the same statement block as the count reset, `revalidateDir` replaces `entries`
  alongside the counts, and each virtual view supplies its own override list. No hole; nothing to fix.
- `selectedCount` / `selectedSize` — selection-derived (reset on navigation), not listing-derived.
- `hiddenShown` — a persisted setting. `notice` / `noticeIsError` — transient toast state.
- `diskFree` / `diskTotal` (`updateDiskSpace`) and `git` (`refreshGitStatus`) — **path**-derived, not
  listing-derived, and their guards name Home + archive but NOT smart folder / structured search. Same
  *shape* of partial gate, but not the same class of defect: both describe `currentPath`, which is still a
  real folder while a virtual view is open, so neither makes a false statement about the listing on
  screen. Recorded here as an observation; out of scope for this ticket and not fixed.

