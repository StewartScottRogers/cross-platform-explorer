---
id: CPE-746
title: Agent Watch — full side-by-side diff view on click
type: feature
component: Frontend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-19
epic: CPE-727
estimate: 2-3h
---

## Summary
Child of CPE-727. Clicking a write-touched row or timeline entry (or the inline peek's "expand") opens a
**full side-by-side before/after diff** for that mutation, from the CPE-744 store — the deep view when the
inline peek isn't enough.

## Scope
- A side-by-side (old | new) diff view for a single path's latest write, reusing `diff.ts` and the diff
  styling; scrolls independently, closes back to the explorer.
- Entry points: click on a write annotation / an "expand" from the CPE-745 peek / a timeline entry.
- Handle the empty-before (created file) and large-diff cases gracefully.
- Themed light/dark; no cost when not watching / no diff selected.

## Acceptance
- [ ] Activating a write annotation/timeline entry opens a side-by-side before/after for that file.
- [ ] Created files (empty before) and large diffs render sensibly; the view closes cleanly.
- [ ] Reuses `diff.ts` + theme variables; no regression to normal preview/selection.

## Notes
Prereq: CPE-744. Pairs with CPE-745 (shares the diff store + renderer). Frontend-only.
