---
id: CPE-1840
title: the status bar's two count fields have no staleness coverage, so a stale count can ship unnoticed
type: task
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-20
closed: 2026-08-22
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
  hit, `goBack` passes `useCache=true`). `C:\d`'s `list_dir` is HELD (never resolves).
- **`revalidateDir` update** (2 tests) — the stream's count and `list_dir`'s count differ (0 cached, 4/6
  fresh), so the assertion can name the specific number only a refresh produces.
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
| `ExplorerPane.svelte:379/380` reset to the WRONG value `= 1` (added after review) | 4 failed, 4 passed — both cache tests + both revalidation pre-assertions |

Gates: `npx vitest run` 321 files / 4266 tests passed; `npm run check` 0 errors, 0 warnings.

### 2026-08-22 — review corrections (same branch, same PR #990)

The independent Reviewer approved the PR, reproduced all eight mutations, and found the tests red under
strictly more mutations than the rationale above required — but it also falsified two liveness claims and
found one real hole. All three corrected here; **tests-only, still no production change**.

1. **The `heldListDir` hold is NOT load-bearing** (my earlier "without it the test would have passed" was
   wrong). With both `heldListDir.add("C:\d")` calls deleted, mutations 1+2 still red — they fail at the
   assertion immediately after `backToDrive()`, which runs well inside the 300ms stale-while-revalidate
   window at `ExplorerPane.svelte:450`, so nothing can race in behind it anyway. The hold is kept because
   it makes each test's SECOND, post-400ms assertion non-vacuous and insures the first against timing
   drift. Comments and this log now say that instead.
2. **The differing stream/`list_dir` counts are NOT load-bearing either.** Equalising them keeps the file
   green unmutated and still reds under mutations 3+4; equalised *and* with all four lines deleted, all
   four tests still red, because the cache branch zeroes the count before the assertion so no remembered
   value can survive. Kept for expressiveness ("it picked up 4", not "something non-zero appeared").
3. **One real gap, closed.** A wrong non-zero cache reset was caught for every value except exactly `1`
   on `filteredHidden`: `StatusBar.svelte:76-78` renders a singular sentence at `=== 1` ("1 entry was
   hidden…") and `FILTERED_NOTE` was plural-only, so `filteredHidden = 1` at :379 left that test GREEN
   (`= 7` reds it). `UNREADABLE_NOTE` already spanned both forms, which is why only that side caught it.
   `FILTERED_NOTE` is now `/hidden because (its name|their names) could not be shown safely/`; the `= 1`
   mutation goes from 2 failed to **4 failed**. All eight original mutations re-verified red 1:1 after
   the change.

Gates after the corrections: `npx vitest run` 321 files / 4266 tests passed; `npm run check` 0 errors,
0 warnings.

**Enumeration of the other listing-derived status-bar props** (the AC's fourth box):

- `itemCount` / `totalCount` — genuinely listing-derived and they carry their own view gate
  (`(isHome && !smartFolder && !structuredSearch)`, plus `|| archive` for `totalCount`), but they are
  **structurally immune** to all three staleness paths, for a stronger reason than "replaced in the same
  statement block" (my first, weaker phrasing): they simply ARE the length of the array being rendered —
  `App.svelte:2967/2969` read `visible.length` / `shown.length`, and `visible`/`shown` are what
  `<ExplorerPane>` (`:85/:142/:165`) derives from `entries` / `smartOverride` / `archiveOverride` and puts
  on screen. There is no side-channel scalar that can drift from the list. `filteredHidden` /
  `unreadableCount` are hand-maintained scalars kept BESIDE the array, and that asymmetry is the whole
  defect class. No hole; nothing to fix.
- `selectedCount` / `selectedSize` — selection-derived (reset on navigation), not listing-derived.
- `hiddenShown` — a persisted setting. `notice` / `noticeIsError` — transient toast state.
- `diskFree` / `diskTotal` (`updateDiskSpace`) and `git` (`refreshGitStatus`) — **path**-derived, not
  listing-derived, and their guards name Home + archive but NOT smart folder / structured search.
  **My original reasoning for calling that harmless was backwards** and is retracted: I argued both still
  describe `currentPath`, "which is still a real folder", so nothing false is on screen. The Reviewer
  showed the guards do the OPPOSITE of what that predicts — while a smart folder or structured search is
  open the breadcrumb reads `Home / <name>` and `currentPath` is not on screen at all (so an ungated disk
  or branch readout is attributed to a view that doesn't name a folder), whereas in an archive — which IS
  guarded — the breadcrumb still contains `splitPath(currentPath)`. It also judged the git case worse than
  either of us thought. Filed by the coordinator as its own ticket; **not widened into this one**, and
  this entry exists so the old rationale isn't left standing for a future maintainer to trust.

