---
id: CPE-1854
title: the git chip's guard is effectively non-reactive, so it goes stale even in an archive
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-22
closed: 2026-08-22
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
`suppressed` as an argument. `updateDiskSpace` was converted to the same signature for symmetry.

**How strong that actually is — corrected.** The first draft of this log claimed the failure mode was
"designed out". The independent Reviewer tested the claim three ways instead of accepting it, and the
honest answer is that the defence is **layered, not absolute**:

| Degradation | Compiler | Suite |
|---|---|---|
| **Omission** — drop the argument | **hard failure**: `Expected 2 arguments, but got 1` at `App.svelte:1250` and `:1665` | — |
| **Literal** — pass `false` instead of the flag | type-checks fine | caught, 8/8 red |
| **Un-tracking by shape** — turn `pathReadoutsSuppressed` into an arrow function called at each site, removing every identifier from both reactive statements (the exact silent shape of the original bug) | type-checks fine | caught, 8/8 red |

So: the compiler catches deletion, the suite catches a wrong value and a wrong shape. **Residual, named
rather than hidden:** both function bodies still read the flag out of scope after the await
(`App.svelte:1244` and `:1673`). That is correct and necessary — a post-await read must be LIVE, not the
value captured when the request was issued — but it means a future arm added only inside a body, without
going through `pathReadoutsSuppressed`, would still be silent.

**A third defect, found while fixing the first two and not in the ticket.** Both fetches are async and
neither re-checked its assumptions at RESOLVE time. `updateDiskSpace` re-checked `currentPath === path`,
which is not sufficient: opening a smart folder or a structured search does not change `currentPath`, so
a response still in flight when the view opened landed and repainted a readout the guard had already
blanked. Both now re-read the live flag after the await.

**And a fourth, which is the most user-visible thing in this change — found by the independent UAT.**
`refreshGitStatus` had **no stale-response check at all** before this ticket, so the bug is not confined
to virtual views: it breaks **ordinary folder-to-folder navigation**. Navigate away from a slow
repository (a large one, or one on a network share) before its `forge_repo_status` returns, and the
FIRST folder's branch repaints over the second folder's — next to live Pull and Push buttons. The UAT
measured it on a throwaway probe rather than reasoning about it: held folder A's status, navigated to B,
released A, and watched A's branch land on top of B. The `currentPath !== path` half of the new
resolve-time check is what stops it. Its disk twin was already correct (`updateDiskSpace` has always
re-checked `currentPath === path`) but was pinned by nothing, which is precisely how the git side shipped
without the equivalent line — so it now has a regression guard too. **Ten tests, not eight.**

**Pull/Push (the last AC).** The answer is structural rather than a new disabled state: `StatusBar.svelte`
renders the branch name, the ahead/behind counts, the dirty dot AND the Pull/Push/Sync…/Resolve… buttons
inside one `{#if git && git.is_repo}` block, so a null `gitStatus` removes the actions along with the
statement they act on. Every git test below asserts `Pull`, `Push` and `Sync…` are absent, not just the
branch name. `maybeAutoSync`'s own guard was moved onto `pathReadoutsSuppressed` too, so the background
mirror cannot run against a folder that is out of view (belt-and-braces — `gitStatus` is null there now,
which its next line already refuses on).

**Every `$:` in `App.svelte`, enumerated.** 47 statements after this change (46 before, plus
`pathReadoutsSuppressed`). The true breakdown is **34 derivations, 8 bare calls, 5 blocks/`if`s** — the
first draft of this log said 35/12, and its "derivations" bucket silently contained three statements that
are not derivations at all (`:1590` `$: if (!activeWatchCwd) showTimeline = false;`, `:2146`, `:3042`) and
so were never walked. They are folded into the table below; all three read every identifier inline and
call no state-reading function, so the conclusion is unchanged — but the coverage argument as originally
written did not reach them.

**The screening rule, corrected.** "A `$: x = <expr>` derivation cannot have this bug" is **overbroad as a
general rule**: `$: x = f()` where `f` reads a Svelte store via `get()` has precisely this bug, because
`get()` is an untracked read. What is actually defensible, and what the Reviewer verified rather than
assumed, is the narrower claim: **no derivation in this file calls a helper that reads a store.** Every
helper invoked from a derivation RHS was checked — `commandsForSurface`, `bindingsForSurface`,
`watchTargetFor`, `recentActivities`, `smartFolderPaths`, `vaultOfSessionPath`, `detectContexts`,
`tagCounts`, `resolveSavedSearchRoot` — and none reads a store; the only `get(` hits in them are
`Map.get`. So the derivations are clean **today, in this file**, not clean by construction.

The 13 call/block/`if` statements, checked individually (line numbers and statement text both post-fix):

| Line | Statement | Verdict |
|------|-----------|---------|
| 292 | `setDiagnosticsEnabled(diagnostics)` | correct — `diagnostics.ts` fn, reads only its parameter |
| 1078 | `$: { … }` navState reset block | correct — reads `activeId`/`activePane` INLINE, both tracked |
| 1098 | `paletteCommands = […]` | correct by design — the wrapper-fn comment above it explains that reading `selectedEntries`/`activeId` inline would form a dependency CYCLE; the closures run on click, not on recompute, so they are not dependencies at all |
| 1250 | `refreshGitStatus(currentPath, pathReadoutsSuppressed)` | **THE BUG — fixed.** Was `$: refreshGitStatus(currentPath);` with the guard reading `isHome`/`archive` from the body |
| 1590 | `if (!activeWatchCwd) showTimeline = false;` | correct — `activeWatchCwd` read inline; assigns only |
| 1401 | `appOrder = (() => {…})()` | correct — IIFE, `$t` read inline |
| 1586 | `reconcileAgentWatch($agentSessions, currentPath)` | correct — the body's other reads (`armedWatches`, `reconcileInFlight`, the `unlisten*` handles) are internal bookkeeping, not view-state guards; nothing about WHICH sessions to watch is read untracked |
| 1665 | `updateDiskSpace(currentPath, pathReadoutsSuppressed)` | was `$: updateDiskSpace(currentPath, isHome, !!archive)` — already right on reactivity, **missing the two virtual-view arms — fixed** |
| 1826 | `loadSmartEntries(smartFolder, smartPaths)` | correct — body reads only its two parameters |
| 1857 | `loadStructuredSearchEntries(structuredSearch)` | **same shape, and harmless — but NOT for the reason first recorded here.** The body reads `currentPath` untracked via `resolveSavedSearchRoot(s, currentPath)`. The first draft justified that by claiming a `currentPath` dependency "would re-run the saved search on navigation and defeat the captured root". **That is wrong**, and the Reviewer caught it: `loadPath` sets `structuredSearch = null` at `App.svelte:2173`, so `currentPath` **cannot change while a search is open**. Adding the dependency would be inert, not harmful. The omission is kept because it is the honest expression of a value only read at open time — not because tracking it would break anything. Same correction applies to the paired claim about `$: smartFolderScope` (two lines down) listing `currentPath` for the same call: with `currentPath` frozen while a search is open, the two can never disagree, so that was a distinction without a behavioural difference |
| 1974 | `archivePreviewResolver.update(archive, selectedEntries)` | correct — `archivePreview.ts`'s `update` takes both inputs as parameters and keeps its own request-id counter |
| 2121 | `manageSmartFolderLiveRefresh(smartFolderScope)` | correct — `smartFolderScope` inside the listener callback is deliberately read LIVE (a debounced fire must recompute whichever folder is open NOW, per its doc); `reconcileWatch`'s reads of `watchLive`/`watchedFolders` are imperatively re-driven from `applyWatchConfig` and the rules editor, so no arm is reachable only through this statement |
| 2146 | `if (!isHome \|\| selectedEntries.length > 0) homePreview = null;` | correct — both identifiers read inline; assigns only |
| 3042 | `if (selection.lead >= 0 && rowEls[selection.lead]) { … }` | correct — reads `selection`/`rowEls` inline, and the body calls only `scrollIntoView` |
| 5743 | `if (sessionReady && autoRestore) { void [tabs, currentPath, view, sortKey, sortDir, search]; … }` | correct, and the pre-existing precedent for the fix: it names its dependencies explicitly in a `void [...]` because `captureCurrentTabs()` reads them from the body |

One adjacent item deliberately NOT changed: `reconcileWatch` also reads `aiConsoleAvailable`, which is
set once during startup probing, so a watcher armed before the probe lands is not re-armed by it. That is
a different feature (CPE-794 watch arming), not a status-bar false statement, and is out of scope here.

**Tests** — `src/App.statusBarPathReadouts.test.ts`, **10 tests**: git chip × {archive, smart folder,
structured search}, disk figures × the same three, the two virtual-view late-result races, and the two
ordinary **folder-to-folder** stale-repaint cases. Each is a before/after pair inside one render (assert
the readout IS there in a real folder, then enter the view and assert it is gone), so an absence
assertion can never pass vacuously. The folder-to-folder pair asserts something stronger than absence:
the two folders carry deliberately different branch names (`hotfix/drive-root-7` vs `release/ledger-9`)
and different free-space figures, and `expectGitChipPresent(branch)` asserts the OTHER folder's branch is
absent — so "the right chip" and "a chip" cannot be confused. The gates in the harness were reworked from
one global hold to PER-PATH holds for this, since holding folder A's fetch while folder B's resolves
normally is an ordering a single shared gate cannot express.

**Red-proof, one mutation at a time against the fixed code:**

| # | Mutation | Result |
|---|----------|--------|
| M1 | restore the pre-fix git shape verbatim — guard reads `isHome`/`archive` from the body, `$: refreshGitStatus(currentPath);` | 4 failed / 4 passed — all three git tests + the git late-result test |
| M2 | same shape on the disk side — `$: updateDiskSpace(currentPath);` with the guard reading the flag from the body | 4 failed / 4 passed — all three disk tests + the disk late-result test |
| M3 | `$: pathReadoutsSuppressed = isHome \|\| !!archive;` (drop both virtual-view arms) | 6 failed / 2 passed — the four smart-folder/structured-search tests + both late-result tests; the two archive tests correctly stayed green |
| M4 | `$: pathReadoutsSuppressed = isHome \|\| !!smartFolder \|\| !!structuredSearch;` (drop the archive arm) | 2 failed / 6 passed — exactly the two archive tests |
| M5 | delete `\|\| pathReadoutsSuppressed` from `refreshGitStatus`'s resolve-time re-check | 1 failed / 9 passed — the git smart-folder late-result test only |
| M6 | delete `&& !pathReadoutsSuppressed` from `updateDiskSpace`'s resolve-time re-check | 1 failed / 9 passed — the disk smart-folder late-result test only |
| M7 | delete `currentPath !== path \|\|` from `refreshGitStatus`'s resolve-time re-check — i.e. the pre-ticket state of that line, which had no path check at all | 1 failed / 9 passed — the git folder-to-folder test only, with `hotfix/drive-root-7` on screen while the user is in `C:\d\photos` |
| M8 | delete `currentPath === path &&` from `updateDiskSpace`'s resolve-time re-check | 1 failed / 9 passed — the disk folder-to-folder test only |

All eight were re-run **after** the harness rework (per-path gates, per-path branch names), not just
before it, and the counts above are from that second pass. The GREEN halves are the part that proves
per-arm discrimination and are recorded deliberately: M1 leaves all four disk tests green, M2 leaves all
four git tests green, M3 leaves both archive tests green, M4 reds exactly two. M1–M6 were independently
reproduced by the Reviewer, which restored the tree byte-identical between runs.

M1 is the mutation the ticket names: an identifier removed from the reactive statement with the guard
body left intact. It required restoring the original two-line shape rather than editing one line, because
after this fix that mutation no longer compiles — which is the point.

**Also touched:** `src/lib/bidiEscape.guard.test.ts` — its two recorded line-number lists for `App.svelte`
(`APP_MARKUP_OFFENDERS`, `APP_SCRIPT_BASENAME_ALLOWLIST`) are absolute line numbers, and this change
inserts 35 lines above them. Shifted by exactly +35; the SET of offenders is unchanged, no entry added or
removed. Docs: `src/docs/03-explorer.md` (a new bullet naming both readouts and where they are hidden) and
`src/docs/08-repositories.md` (the same fact for the git indicator, plus why its buttons go with it). No
new section, so `src/lib/sectionDocs.ts` is untouched.

**The taste question — settled, and not by me.** Whether to suppress the DISK figure in a smart folder or
structured search was the one genuinely arguable call in this change (the git chip is not arguable: a
branch name is a claim about a specific repository). The independent UAT went in **expecting to argue for
keeping it** and changed its mind on one piece of evidence, which is better reasoning than mine and is
recorded here rather than paraphrased:

- **Free space is already on screen permanently, in a better form.** `Sidebar.svelte:796-804` renders a
  per-drive usage bar plus "N free" for **every** drive, and the sidebar has no hide toggle. A user in a
  smart folder has therefore not lost free-space awareness — they have lost one figure about a folder they
  cannot see, and kept every drive's figure. In the **multi-drive smart folder** case that is strictly
  better: a status-bar figure would have to pick one drive arbitrarily and would be wrong for most of the
  rows on screen.
- **The consistency argument I could have made and did not:** `App.svelte:6952-6953` ALREADY zeroes
  `filteredHidden` and `unreadableCount` for exactly this four-view set (CPE-1840). Keeping disk would
  have left it the only path-derived readout in the bar that survives into a view that does not describe it.
- **"A blank reads as a bug" is weak here**, because the git chip goes at the same moment: the whole
  right-hand cluster empties, which reads as "this view has no folder facts" rather than "something
  failed". The identical blank already occurs on Home and in every non-repo folder.

**Two further UAT findings, recorded rather than acted on** — both are correct behaviour, and both are the
kind of thing a later reader would otherwise re-litigate:

- **Suppression is not a one-way trip.** Entering an archive and leaving it via Up — where `currentPath`
  never changes — brings both readouts back from a **fresh fetch**, not from a cached value. The UAT
  counted the invocations rather than inferring it.
- **A configured auto-mirror is DEFERRED, not lost,** while the user sits in a smart folder. `isDue` is
  time-based, so the next tick after leaving catches up. This matches the pre-existing design, which
  already stops auto-mirroring on Home.

**One gap the Reviewer named, which I then closed by inspection.** `smartFolder`/`structuredSearch`/
`archive` are App-level rather than per-tab, so the flag's behaviour across a **tab switch while a virtual
view is open** was flagged as unchased. Checked: `selectTab` (`App.svelte:2951-2954`), `newTab` (`:2838`)
and `closeTab` (`:2858`) all route through `loadPath`, which clears all three view flags at `:2172-2180`.
A tab switch therefore always exits the virtual view and re-fetches for the new tab's path — the App-level
flags cannot desynchronise from the active tab. Verified by reading the call graph, **not** by a test; a
test for it would belong to the tab feature, not to this ticket.

**Not mine — being filed separately, deliberately not widened into this diff.** (a) `.disk` has no
right-anchor of its own, only `margin-left: 12px`; it sits at the right edge purely because `.git`
precedes it carrying `margin-left: auto`. So on leaving an archive back into a git repo, both readouts
refetch independently, and if the fast `disk_space` resolves before the slow `forge_repo_status` the
free-space text renders next to the item count and then JUMPS right when the chip lands. Sub-second, and
pre-existing in the CSS — but this change creates the path that exposes it, because the chip never used to
be cleared on entering those views. (b) The sidebar's per-drive usage bars are filled on mount and on
drive-list change only (`loadDriveUsage`, `App.svelte:1606`), so they can be hours stale — which is worth
knowing given the taste argument above leans on them.

**Could not verify.** jsdom does not apply component CSS under this project's vitest config, so
`getComputedStyle` reports nothing useful and nothing here checks layout, ordering, truncation or where in
the bar the readouts sit — every assertion is presence/absence of TEXT. Not verified on the real app
either: no GUI run was done for this ticket, so the live-probe observations from CPE-1840 (stale branch in
structured search; chip present in an archive) are reproduced as unit tests rather than re-measured on
screen.

**Gates.** `npx vitest run` — 325 files, 4321 tests, all passing (4319 before the two folder-to-folder
tests). `npm run check` — 0 errors, 0 warnings. Both re-run and independently confirmed by the Reviewer,
which also re-derived the `bidiEscape` +35 shift mechanically (31 entries both sides, identical expression
sequence and order, every delta exactly +35, every recorded line landing on its recorded expression in its
own file version) and checked line endings itself: `App.svelte` 7880 CRLF / 0 bare LF / no BOM, all four
other changed files 0 bare LF, with `.gitattributes` carrying no rule for these paths — so that is real
file state, not normalisation. Worth noting because the LF hazard was hit twice mid-ticket: once by
`git checkout -- src/App.svelte` discarding the uncommitted fix, and once by a `python -c "…"` heredoc
whose backticks and backslashes were eaten by bash. Both caught by `git diff --numstat` plus a byte-level
line-ending count; the final edits were made with the Edit tool.
