---
id: CPE-1859
title: the disk readout is right-aligned only by accident of the git chip preceding it
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-22
closed:
---

## Problem

`.disk` in the status bar has **no right-anchor of its own** — only `margin-left: 12px`. It sits at the
right edge purely because `.git` precedes it carrying `margin-left: auto`.

So when the git chip is absent while the disk figure is present, the free-space text renders
**left-adjacent to the item count** instead of at the right edge.

That window is short but real, and CPE-1854 created a path that exposes it. Leaving an archive back into
a git repository refetches both readouts independently; `disk_space` is fast and `forge_repo_status` is
slow on a large repository or a network share. So the sequence is: disk lands and renders in the wrong
place, then the chip lands and the disk figure **jumps** to the right edge.

Sub-second, and pre-existing in the CSS rather than introduced by CPE-1854 — but before that ticket the
chip was never cleared on entering those views, so the path did not exist.

## A second, related staleness

The sidebar's per-drive usage bars (`Sidebar.svelte:796-804`) are filled on mount and on drive-list change
only (`loadDriveUsage`, `App.svelte:1606`). They can be hours stale.

That matters more than it used to: CPE-1854's UAT justified hiding the status-bar disk figure in virtual
views partly on the grounds that free space is *already on screen permanently* in the sidebar. That
argument is sound, and it makes the sidebar the primary free-space readout — which is worth it being
fresh, or at least refreshed on a coarse timer.

## Acceptance criteria

- [ ] `.disk` anchors itself rather than relying on a sibling. `margin-left: auto` on `.disk` is the
      obvious fix; confirm it does not change the layout when both are present.
- [ ] Verify the fix in a real render, not in jsdom. This project's vitest config applies **no component
      CSS** to `getComputedStyle`, so a unit test cannot see this class of defect at all — that is exactly
      why it survived. Use the gui-smoke harness or a screenshot, and say which.
- [ ] Check the same shape for every other status-bar item that has no anchor of its own and relies on a
      neighbour's `auto` margin. Enumerate the row rather than fixing only `.disk`.
- [ ] Decide whether the sidebar drive bars should refresh on a timer or on navigation, and record the
      cost either way. If the answer is no, say what keeps them honest.

## Notes

Found by the independent UAT during CPE-1854, which flagged it as reasoned-from-markup rather than
measured: it could read `app.css:1451` and `StatusBar.svelte:135-154` but could not verify any of it in
jsdom. It also confirmed the parts that are *not* a problem — `.statusbar` is a fixed `height: 26px` so
there is no vertical jump, and the left-hand items do not shift because `.git` carries the only
`margin-left: auto`.

Related: CPE-1854 (created the exposing path), CPE-1836 (the row's layout at the 600px floor), CPE-1840
(the two count fields in the same row).
