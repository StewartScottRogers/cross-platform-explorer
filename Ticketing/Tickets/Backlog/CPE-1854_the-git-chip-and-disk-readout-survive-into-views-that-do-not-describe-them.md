---
id: CPE-1854
title: the git chip's guard is effectively non-reactive, so it goes stale even in an archive
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-22
closed:
---

## Problem

Two status-bar readouts describe `currentPath` — the git branch chip and the free/total disk figures.
Both are supposed to be suppressed in views where that path is not what the user is looking at. Neither
guard does what it claims, and the git one is broken in a way that is easy to miss.

Measured by the independent Reviewer during CPE-1840, on a live probe:

- **Structured search** — the git chip still shows the **previous folder's branch**, and the disk readout
  still shows the previous folder's figures. Both stale.
- **Archive** — disk correctly cleared, but the **git chip is still present**.
- **Smart folder** — inferred from the code, not directly observed (the probe's own fixture had a bug and
  was not re-chased). Neither `smartFolder` nor `structuredSearch` appears in either guard or either
  dependency list, so the static reading is unambiguous.

## The git case is a reactivity bug, not just a missing arm

`App.svelte:1241` is `$: refreshGitStatus(currentPath);`. Svelte tracks only the identifiers appearing in
the reactive statement, so `isHome` and `archive` — referenced inside the guard at `:1233` — are **not
dependencies**. The `archive` arm therefore never fires on *entering* an archive; it only takes effect on
the next path change.

`App.svelte:1653` is `$: updateDiskSpace(currentPath, isHome, !!archive)`, which does list `archive` —
which is exactly why disk behaves on entering an archive and git does not. The two are one character of
discipline apart.

## Why the original rationale was backwards

CPE-1840's worker recorded these as a *different class* of defect, reasoning that both readouts "describe
`currentPath`, which is still a real folder while a virtual view is open."

The breadcrumb code contradicts that. `App.svelte:2955-2963`: while a **smart folder or structured search**
is open the breadcrumb reads `Home / <name>` and `currentPath` is **not on screen anywhere**; in an
**archive** the breadcrumb still contains `...splitPath(currentPath)...`. So the guards null the readouts
in the one view where the path *is* still visible, and leave them live in the two where it is not — the
opposite of what the rationale predicts.

## Why it matters more than a stale number

The git widget carries live **Pull / Push buttons** (`App.svelte:6924-6925`). A stale branch chip is not
just a false statement; it is a false statement next to two actions.

This is the same false-statement shape as CPE-1708, CPE-1780 and CPE-1840 — the app quietly describing
something other than what is on screen.

## Acceptance criteria

- [ ] `refreshGitStatus`'s reactive statement lists every identifier its guard reads, so the guard fires on
      entering a view rather than on the next path change. Check every other `$:` in `App.svelte` for the
      same shape and report what you find — this is a whole class, not one line.
- [ ] Both `git` and `diskFree`/`diskTotal` are suppressed in **archive, smart folder and structured
      search**, or each exception is justified against what the breadcrumb actually shows in that view.
- [ ] A test per view per readout — six — asserting the readout is absent. CPE-1840 established that a
      single test covering one arm is what leaves the others uncovered.
- [ ] Red-proof each with the minimal realistic change: for the reactivity bug specifically, the mutation is
      removing an identifier from the reactive statement while leaving the guard body intact, since that is
      the shape that fails silently.
- [ ] The Pull/Push buttons must not be actionable against a branch the chip is no longer describing.

## Notes

Found by the independent Reviewer during CPE-1840's review, which recommended a ticket rather than widening
a tests-only PR. CPE-1840 pinned the two count fields; this is the same audit one field over.

Related: CPE-1840 (the counts), CPE-1836 (the row's layout at the 600px floor), CPE-1833 (neither note is
announced to a screen reader).

## Work Log

### 2026-08-22 — branch `cpe-1854-git-chip-reactivity`

**The fix.** One derived boolean now carries the whole suppression decision and is PASSED IN to both
guards, so the dependency list and the guard body cannot drift apart:

```
$: pathReadoutsSuppressed = isHome || !!archive || !!smartFolder || !!structuredSearch;
$: refreshGitStatus(currentPath, pathReadoutsSuppressed);
$: updateDiskSpace(currentPath, pathReadoutsSuppressed);
```

`refreshGitStatus` no longer reads `isHome`/`archive` out of the surrounding scope; it takes
`suppressed` as an argument. That is what makes the bug non-reproducible by omission: delete the
identifier from the reactive statement and the call no longer type-checks, instead of silently
never firing. `updateDiskSpace` was converted to the same signature for symmetry.

**A third defect, found while fixing the first two and not in the ticket.** Both fetches are async and
neither re-checked suppression at RESOLVE time. `updateDiskSpace` re-checked `currentPath === path`,
which is not sufficient: opening a smart folder or a structured search does not change `currentPath`, so
a response still in flight when the view opened landed and repainted a readout the guard had already
blanked. Both now re-read the live flag after the await. Two extra tests cover it (8 total, not 6).

**Pull/Push (the last AC).** The answer is structural rather than a new disabled state: `StatusBar.svelte`
renders the branch name, the ahead/behind counts, the dirty dot AND the Pull/Push/Sync…/Resolve… buttons
inside one `{#if git && git.is_repo}` block, so a null `gitStatus` removes the actions along with the
statement they act on. Every git test below asserts `Pull`, `Push` and `Sync…` are absent, not just the
branch name. `maybeAutoSync`'s own guard was moved onto `pathReadoutsSuppressed` too, so the background
mirror cannot run against a folder that is out of view (belt-and-braces — `gitStatus` is null there now,
which its next line already refuses on).

**Every `$:` in `App.svelte`, enumerated.** 47 statements after this change (46 before, plus
`pathReadoutsSuppressed`). Thirty-five are `$: x = <expr>` derivations, which list every identifier they
read by construction and therefore cannot have this bug — the failure mode only exists for a statement
that CALLS A FUNCTION whose body reads reactive state the statement does not name. The remaining twelve
call/block statements were checked individually (line numbers post-fix):

| Line | Statement | Verdict |
|------|-----------|---------|
| 292 | `setDiagnosticsEnabled(diagnostics)` | correct — `diagnostics.ts` fn, reads only its parameter |
| 1078 | `$: { … }` navState reset block | correct — reads `activeId`/`activePane` INLINE, both tracked |
| 1098 | `paletteCommands = […]` | correct by design — the wrapper-fn comment above it explains that reading `selectedEntries`/`activeId` inline would form a dependency CYCLE; the closures run on click, not on recompute, so they are not dependencies at all |
| 1250 | `refreshGitStatus(currentPath)` | **THE BUG — fixed** |
| 1401 | `appOrder = (() => {…})()` | correct — IIFE, `$t` read inline |
| 1586 | `reconcileAgentWatch($agentSessions, currentPath)` | correct — the body's other reads (`armedWatches`, `reconcileInFlight`, the `unlisten*` handles) are internal bookkeeping, not view-state guards; nothing about WHICH sessions to watch is read untracked |
| 1665 | `updateDiskSpace(currentPath, isHome, !!archive)` | was already right on reactivity, **missing the two virtual-view arms — fixed** |
| 1826 | `loadSmartEntries(smartFolder, smartPaths)` | correct — body reads only its two parameters |
| 1857 | `loadStructuredSearchEntries(structuredSearch)` | **same shape, deliberately** — the body reads `currentPath` untracked via `resolveSavedSearchRoot(s, currentPath)`. That is the documented "falls back to the currently-open folder AT OPEN TIME" fallback: making `currentPath` a dependency would re-run the saved search on navigation, which is the opposite of what a captured root means. Left as-is, recorded here so the next audit does not re-open it. (`$: smartFolderScope` two lines down DOES list `currentPath` for the same call — correct there, because that one feeds the OS watcher's path set, which must follow navigation.) |
| 1974 | `archivePreviewResolver.update(archive, selectedEntries)` | correct — `archivePreview.ts`'s `update` takes both inputs as parameters and keeps its own request-id counter |
| 2121 | `manageSmartFolderLiveRefresh(smartFolderScope)` | correct — `smartFolderScope` inside the listener callback is deliberately read LIVE (a debounced fire must recompute whichever folder is open NOW, per its doc); `reconcileWatch`'s reads of `watchLive`/`watchedFolders` are imperatively re-driven from `applyWatchConfig` and the rules editor, so no arm is reachable only through this statement |
| 5743 | `if (sessionReady && autoRestore) { void [tabs, currentPath, view, sortKey, sortDir, search]; … }` | correct, and the pre-existing precedent for the fix: it names its dependencies explicitly in a `void [...]` because `captureCurrentTabs()` reads them from the body |

One adjacent item deliberately NOT changed: `reconcileWatch` also reads `aiConsoleAvailable`, which is
set once during startup probing, so a watcher armed before the probe lands is not re-armed by it. That is
a different feature (CPE-794 watch arming), not a status-bar false statement, and is out of scope here.

**Tests** — `src/App.statusBarPathReadouts.test.ts`, 8 tests: git chip × {archive, smart folder,
structured search}, disk figures × the same three, plus the two late-result races. Each is a
before/after pair inside one render (assert the readout IS there in a real folder, then enter the view
and assert it is gone), so an absence assertion can never pass vacuously.

**Red-proof, one mutation at a time against the fixed code:**

| # | Mutation | Result |
|---|----------|--------|
| M1 | restore the pre-fix git shape verbatim — guard reads `isHome`/`archive` from the body, `$: refreshGitStatus(currentPath);` | 4 failed / 4 passed — all three git tests + the git late-result test |
| M2 | same shape on the disk side — `$: updateDiskSpace(currentPath);` with the guard reading the flag from the body | 4 failed / 4 passed — all three disk tests + the disk late-result test |
| M3 | `$: pathReadoutsSuppressed = isHome \|\| !!archive;` (drop both virtual-view arms) | 6 failed / 2 passed — the four smart-folder/structured-search tests + both late-result tests; the two archive tests correctly stayed green |
| M4 | `$: pathReadoutsSuppressed = isHome \|\| !!smartFolder \|\| !!structuredSearch;` (drop the archive arm) | 2 failed / 6 passed — exactly the two archive tests |
| M5 | delete `\|\| pathReadoutsSuppressed` from `refreshGitStatus`'s resolve-time re-check | 1 failed / 7 passed — the git late-result test only |
| M6 | delete `&& !pathReadoutsSuppressed` from `updateDiskSpace`'s resolve-time re-check | 1 failed / 7 passed — the disk late-result test only |

M1 is the mutation the ticket names: an identifier removed from the reactive statement with the guard
body left intact. It required restoring the original two-line shape rather than editing one line, because
after this fix that mutation no longer compiles — which is the point.

**Also touched:** `src/lib/bidiEscape.guard.test.ts` — its two recorded line-number lists for `App.svelte`
(`APP_MARKUP_OFFENDERS`, `APP_SCRIPT_BASENAME_ALLOWLIST`) are absolute line numbers, and this change
inserts 35 lines above them. Shifted by exactly +35; the SET of offenders is unchanged, no entry added or
removed. Docs: `src/docs/03-explorer.md` (a new bullet naming both readouts and where they are hidden) and
`src/docs/08-repositories.md` (the same fact for the git indicator, plus why its buttons go with it). No
new section, so `src/lib/sectionDocs.ts` is untouched.

**Could not verify.** jsdom does not apply component CSS under this project's vitest config, so
`getComputedStyle` reports nothing useful and nothing here checks layout, ordering, truncation or where in
the bar the readouts sit — every assertion is presence/absence of TEXT. Not verified on the real app
either: no GUI run was done for this ticket, so the live-probe observations from CPE-1840 (stale branch in
structured search; chip present in an archive) are reproduced as unit tests rather than re-measured on
screen.

**Gates.** `npx vitest run` — 325 files, 4319 tests, all passing. `npm run check` — 0 errors, 0 warnings.
