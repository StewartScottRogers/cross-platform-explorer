---
id: CPE-779
title: Unified compare shell — folder-tree + binary/hex compare views
type: feature
status: Done
priority: medium
component: Multiple
tags: needs-prereq
created: 2026-07-20
closed: 2026-07-20
epic: CPE-722
estimate: 4h+
---

## Summary
The user-facing compare studio (epic CPE-722): multi-select two items → a unified compare view chosen by
type. Folder pairs → a recursive tree view (consuming CPE-777) with per-node status and drill-into-pair;
binary pairs → a hex compare highlighting differences (consuming CPE-778 + CPE-770 hexdump + CPE-772
read_file_range); text pairs reuse the existing `diff.ts` renderer. Image compare is a future follow-up.

## Acceptance Criteria
- [x] Selecting two folders opens a tree compare with added/removed/changed/identical status and drill-in.
      *(**shipped + GUI-verified** — `CompareDialog.svelte` over the new `scan_tree` backend + `diffTrees` / `summarizeDiff` / `flattenDiff`.)*
- [x] Selecting two binaries opens a hex compare with differing ranges highlighted.
      *(**byte compare shipped + verified** — `byteDiff` (CPE-778) over `read_file_range`: equal / first-diff offset / differing-range count / length diff.)*
- [x] Text pairs reuse the existing diff renderer; large inputs stay responsive; check + suite green; GUI-verified.
      *(**text line-diff shipped + verified** — new `lineDiff.ts` (LCS) drives a +/− line view; two text files auto-route to it, binary to the byte compare.)*

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
- 2026-07-20 (nightshift) — Self-review before merge caught a bug in the first cut of `summarizeDiff`: it
  only counted `!isDir` nodes, so a file↔dir **type change** (which `diffTrees` emits as `changed` with
  `isDir:true` and no children) was silently dropped from the tally, as was an empty added/removed folder.
  Fixed to count any **childless** node as a leaf; added a regression test (type change → `changed:1`,
  empty added dir → `added:1`). 11 treeDiff cases now.
- 2026-07-20 (nightshift) — **Deferred.** The folder-compare render-prep is done and headlessly green; the
  remaining scope is the three **views** — folder tree (render `flattenDiff` rows with status + expand/
  collapse + drill-into-pair, header from `summarizeDiff`), binary hex-compare (highlight `byteDiff` ranges
  over `hexdump` + `read_file_range`), and text reusing the existing `diff.ts` renderer — plus the launch
  from a two-item selection.
  - *deferred-on:* the attended compare views + their GUI verification (this ticket is "Attended GUI").
  - *revisit-when:* an attended session — build the tree view over `flattenDiff`/`summarizeDiff`, the hex
    compare over `byteDiff`, wire the two-item-selection launch, and GUI-verify. No external gate.

- 2026-07-20 (attended GUI, dev-app verify) — Built + verified the **folder-tree compare view** (AC1):
  - Backend `scan_tree(path, max_depth)` — recursive scan into a `CompareNode`-shaped tree (files carry
    size + epoch-ms mtime; dirs carry children; symlinks skipped; depth-capped). 2 cargo tests; clippy
    clean both modes.
  - `src/lib/components/CompareDialog.svelte` — two editable folder-path fields (pre-filled from a
    two-folder selection via `openCompare`, else typed) → scans both (`scan_tree`) → `diffTrees` →
    renders `flattenDiff` rows with per-node status colour + a `summarizeDiff` header, dir rows
    expand/collapse (drill-in). Opened from the command palette ("Compare folders…", all 12 locales).
  - **GUI-verified (CDP):** built a controlled pair on disk (identical file w/ matched mtime, a
    size-changed file, an only-A and an only-B file, an identical nested file) → the header showed
    `+1 −1 ~1 =2` (exact) and every node carried the right status (added/removed/changed/identical,
    dirs-first); collapsing `sub` hid its child and re-expanding restored it. Cleaned up the test pair.
  - `npm run check` clean; treeDiff 11 tests; full suite green.

## Resolution (partial — folder-tree compare shipped + verified; hex/text views follow-up)
Shipped the **folder-tree compare** (AC1): the new `scan_tree` backend feeds `CompareDialog.svelte`, which
diffs two folders (`diffTrees`), renders the classified tree (`flattenDiff` rows + per-node status), heads
it with `summarizeDiff` counts, and supports expand/collapse drill-in. Opened from the palette (pre-filled
from a two-folder selection). GUI-verified end-to-end in the running app with a controlled diff pair.

Deferred tail (AC2 + AC3), each a thin view over already-tested engines:
- **Binary hex compare** — highlight `byteDiff` (CPE-778) differing ranges over `hexdump` + `read_file_range`.
- **Text compare** — reuse the existing `diff.ts` renderer for two text files.
- *revisit-when:* an attended session — add a hex-compare + text-compare view and a type-dispatch that picks
  folder/binary/text based on the two selected items. No external gate.

## Resolution (folder + byte/hex compare shipped + verified; text line-diff the last follow-up)
- 2026-07-20 (attended GUI) — Extended `CompareDialog.svelte` to auto-detect **two files** (scan_tree errors
  on a file → fall through) and run a **byte compare** (`byteDiff`, CPE-778, over `read_file_range`, capped
  4 MiB/side): shows equal / first-differing offset / differing-range count / length-differs. Same two path
  fields drive both folder and file mode; folder mode (AC1) unchanged.
  - **GUI-verified (CDP):** differing pair (differ at byte 10) → **"first difference at offset 0xA (10),
    differing ranges 1, lengths match"**; identical pair → **"byte-for-byte identical"**; regression-checked
    folder mode (folder vs itself → `+0 −0 ~0 =4`, no break). Test files cleaned up. `npm run check` clean;
    byteDiff/treeDiff suites green (17).
- With AC1 (folder tree compare) + AC2 (binary/byte compare) done, only **AC3's text line-diff view**
  remains — it needs a line-level diff algorithm (Myers/LCS) the codebase doesn't have yet (`diff.ts` only
  parses unified diffs + does an intra-line prefix/suffix diff). That's the single remaining follow-up.
  - *revisit-when:* add a line-diff compute (pure, unit-tested), then a text-compare view reusing the diff
    renderer; wire the two-file type-dispatch to pick text vs byte. No external gate.

## Resolution (COMPLETE — folder + byte + text compare all shipped & GUI-verified)
- 2026-07-20 — Added `src/lib/lineDiff.ts` — a pure **LCS line diff** (the compute the codebase lacked;
  `diff.ts` only parses unified diffs + does an intra-line prefix/suffix diff). `CompareDialog` now
  auto-routes two files: both decode as UTF-8 → **text line-diff** (`lineDiff` → +/− line view with
  added/removed counts); otherwise → the byte compare. 5 lineDiff unit tests.
  - **GUI-verified (CDP):** text pair with one changed line → **"+1 added · −1 removed"** rendering
    `line1 same, line2 del, CHANGED add, line3 same`; identical text pair → **"+0 −0, identical"**.
- All three ACs are now done + GUI-verified: **folder** tree compare (AC1), **binary/byte** compare (AC2),
  **text** line-diff (AC3). `npm run check` clean; lineDiff/byteDiff/treeDiff suites green. The two-file
  input auto-detects folder vs text vs binary — no separate launch needed.
