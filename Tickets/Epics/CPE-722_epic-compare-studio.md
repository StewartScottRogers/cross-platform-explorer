---
id: CPE-722
title: "EPIC: Compare studio (image / binary / folder-tree)"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
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
