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
