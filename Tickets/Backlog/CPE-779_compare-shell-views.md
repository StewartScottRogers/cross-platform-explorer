---
id: CPE-779
title: Unified compare shell — folder-tree + binary/hex compare views
type: feature
status: Open
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
- [ ] Selecting two folders opens a tree compare with added/removed/changed/identical status and drill-in.
- [ ] Selecting two binaries opens a hex compare with differing ranges highlighted.
- [ ] Text pairs reuse the existing diff renderer; large inputs stay responsive; check + suite green; GUI-verified.

## Notes
Prereq: CPE-777, CPE-778. Attended GUI. Launch from a two-item selection (context menu / command).
