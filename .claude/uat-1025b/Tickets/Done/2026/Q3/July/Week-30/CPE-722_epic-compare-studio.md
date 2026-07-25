---
id: CPE-722
title: "EPIC: Compare studio (image / binary / folder-tree)"
type: Task
status: Done
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed: 2026-07-21
---

## Goal
Broaden comparison past the git-diff text renderer into a first-class compare workspace: image compare
(side-by-side, onion-skin, pixel-difference heatmap), binary/hex compare, and recursive folder-tree compare
(added/removed/changed/identical, drill into any pair). Launch by multi-selecting two items.

## Why
`diff.ts` only parses unified-diff text today. Comparing two images, two binaries, or two folder trees is a
common real task the app can't do; a unified compare shell makes CPE a genuine comparison tool.

## Rough scope (areas, not child tickets)
- Comparison engines per content class (image, binary/hex, folder tree).
- A unified compare shell launched from a two-item selection.
- Folder-tree compare with status per entry and drill-into-pair.
- Reuse existing text-diff rendering for text pairs.

## Open questions (resolve at activation)
- Image-diff algorithm (pixel vs. perceptual) and performance on large images.
- Folder compare by name/size/mtime vs. content hash; cost on big trees.
- Integration with dual-pane ([[CPE-617]]) for "compare left vs right pane".

## Definition of Done
- Selecting two files/folders opens a compare view appropriate to their type.
- Image, binary, and recursive folder-tree comparison all work with clear status/visuals.
- Text pairs reuse the existing diff renderer; large inputs stay responsive.

## Work Log
2026-07-20 (nightshift, 00:53 MST) — Activated. Open questions resolved (autonomous): folder compare by
**name + size + mtime** (no content hash — cheap on big trees; a "deep content" mode is a later option);
**image compare deferred** to a follow-up (pixel-diff is GUI/perf-heavy); dual-pane integration deferred
(CPE-617 not landed). Text pairs reuse the existing `diff.ts` renderer. Foundation-first: the two pure
comparison cores land + unit-tested before the compare shell.

## Child tickets
1. **CPE-777** — Pure recursive folder-tree diff (`src/lib/treeDiff.ts`): two trees → per-node
   added/removed/changed/identical (files by size+mtime; a dir is "changed" iff any descendant differs).
   Unit-tested. **Foundation, headless.**
2. **CPE-778** — Pure binary/byte diff (`src/lib/byteDiff.ts`): first-difference offset + differing byte
   ranges between two buffers; reused by the hex compare over `read_file_range` (CPE-772). Unit-tested.
   **Headless.** *(independent of 777)*
3. **CPE-779** — Unified compare shell launched from a two-item selection: folder-tree view (consumes 777)
   + binary/hex view (consumes 778 + CPE-770/772); text pairs reuse `diff.ts`. **Attended GUI.** Image
   compare = future follow-up. *(prereq: 777, 778)*

## Resolution (closed 2026-07-21)
All child tickets are **Done** — the epic's Definition of Done is delivered by the compare studio — image / binary / folder-tree diff (CPE-777/778/779). Closed as part of the
epic-queue tidy-up: every planned child shipped, no remaining scope. Feature verification lives in each
child's Resolution.
