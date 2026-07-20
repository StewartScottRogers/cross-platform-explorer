---
id: CPE-779
title: Unified compare shell — folder-tree + binary/hex compare views
type: feature
status: Deferred
priority: medium
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-722
estimate: 4h+
---

## Summary
The user-facing compare studio (epic CPE-722): multi-select two items → a unified compare view chosen by
type. Folder pairs → a recursive tree view (consuming CPE-777) with per-node status and drill-into-pair;
binary pairs → a hex compare highlighting differences (consuming CPE-778 + CPE-770 hexdump + CPE-772
read_file_range); text pairs reuse the existing `diff.ts` renderer. Image compare is a future follow-up.

## Acceptance Criteria
- [~] Selecting two folders opens a tree compare with added/removed/changed/identical status and drill-in.
      *(diff engine `diffTrees` (CPE-777) + **new render-prep** `summarizeDiff` (header counts) & `flattenDiff` (virtualized rows + collapse/drill-in) landed & tested; the tree view that renders them is the attended GUI tail.)*
- [ ] Selecting two binaries opens a hex compare with differing ranges highlighted.
      *(`byteDiff` (CPE-778) + `hexdump` already supply the differing ranges; the hex-compare view is the GUI tail.)*
- [~] Text pairs reuse the existing diff renderer; large inputs stay responsive; check + suite green; GUI-verified.
      *(`npm run check` clean + vitest 10/10 now; the views + **GUI-verified** are attended.)*

## Notes
Prereq: CPE-777, CPE-778. Attended GUI. Launch from a two-item selection (context menu / command).

## Work Log
- 2026-07-20 (nightshift) — Picked up. The diff *engines* already exist and are tested (`diffTrees`
  CPE-777, `byteDiff` CPE-778, `hexdump`/`hexinspect`), so what was missing headlessly is the **render-prep
  for the folder-tree view**: header status counts and a flatten-to-rows with depth + collapse (the same
  shape the grid virtualization consumes). Built those; the three compare views themselves are attended GUI.
- 2026-07-20 (nightshift) — Added to `src/lib/treeDiff.ts`: `summarizeDiff(nodes) -> {added,removed,changed,
  identical}` (counts **files**/leaves by status, recursing into dirs — an added/removed subtree contributes
  its file leaves, dirs aren't counted themselves; this is the compare-header tally) and `flattenDiff(nodes,
  collapsed?, prefix?, depth?)` → `DiffRow[]` ({node, depth, path, hasChildren}) for a virtualized tree
  view, where a dir whose `path` is in `collapsed` yields its own row but not its descendants (drill-in
  without re-diffing). Pure/recursive. 4 vitest cases (mixed+nested+added-subtree summary; empty summary;
  depth-first flatten with path/depth/hasChildren; collapse hides descendants). `npm run check` clean.
- 2026-07-20 (nightshift) — **Deferred.** The folder-compare render-prep is done and headlessly green; the
  remaining scope is the three **views** — folder tree (render `flattenDiff` rows with status + expand/
  collapse + drill-into-pair, header from `summarizeDiff`), binary hex-compare (highlight `byteDiff` ranges
  over `hexdump` + `read_file_range`), and text reusing the existing `diff.ts` renderer — plus the launch
  from a two-item selection.
  - *deferred-on:* the attended compare views + their GUI verification (this ticket is "Attended GUI").
  - *revisit-when:* an attended session — build the tree view over `flattenDiff`/`summarizeDiff`, the hex
    compare over `byteDiff`, wire the two-item-selection launch, and GUI-verify. No external gate.

## Resolution (partial — folder-compare render-prep landed, views deferred)
Added `summarizeDiff` + `flattenDiff` to `src/lib/treeDiff.ts` — the pure render-prep the folder-tree
compare view is a thin layer over: header status counts and a flatten-to-rows (depth + `/`-joined path +
`hasChildren`) that honors a `collapsed` set for expand/collapse & drill-in without re-diffing. With the
existing `diffTrees`/`byteDiff`/`hexdump` engines, the compare *logic* is complete and unit-tested (10
cases in treeDiff); only the three attended views + the two-item-selection launch + GUI verification remain.
Deferred with a turnkey revisit note.
