---
id: CPE-706
title: "EPIC: Disk usage & space analyzer"
type: Task
status: In Progress
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Answer "why is this drive full?" inside the explorer: on-demand recursive folder sizes shown inline as a
sortable column, plus an interactive treemap/sunburst panel with drill-down and delete-from-here.

## Why
The one perennial question a file explorer can't currently answer. A cancellable size walker plus a
visualization turns CPE into a disk-cleanup tool, while respecting the fast-when-off rule (opt-in scan).

## Rough scope (areas, not child tickets)
- A cancellable, streaming recursive size walker in Rust with per-folder cached totals.
- An inline "Size (recursive)" column that fills in as the walk streams.
- A treemap/sunburst visualization pane (canvas/SVG) with drill-down + largest-files surfacing.
- Reveal / delete actions from the visualization, cross-OS sparse/hardlink awareness.

## Open questions (resolve at activation)
- Cache invalidation for folder totals as files change; memory budget for large trees.
- Treemap rendering approach and performance on 100k+ node trees.
- Hardlink/sparse double-counting policy per OS.

## Definition of Done
- A folder's recursive size can be computed on demand, streamed, and cancelled mid-walk.
- The treemap drills into subfolders and surfaces the biggest space consumers, with reveal/delete.
- No background scanning occurs unless the user asks; core listing is unaffected.

## Decisions (activated 2026-07-19 — autonomous, best-guess logged; user delegated PM)

**Research finding: the recursive walk already exists.** `src-tauri/src/lib.rs` already has `dir_size(path)`
(recursive total, symlink-cycle-safe per CPE-611) and `folder_stats(path)` (aggregate, capped at 500k
entries), plus `disk_space`. So the epic does NOT need a new size walker — it needs (a) a *per-child*
breakdown for the treemap, and (b) the frontend surfaces (column + treemap). Scope adjusted accordingly.

- **Backend:** one new command `dir_children_sizes(path)` returning each direct child's recursive size
  (reusing `dir_size` per child), cancellable — the only backend work. `dir_size` reused as-is for the
  single-folder total behind the column.
- **Treemap rendering:** zero-dependency **SVG/canvas** (the CSP forbids external chart libs); cap
  tiles/depth and aggregate the long tail into an "other" tile for responsiveness. Follow the dataviz skill.
- **Fast-when-off:** scanning is strictly opt-in (open the Space view / turn the column on); leaving stops
  any in-flight scan — no background walking, no startup cost (PURPOSE.md tiebreaker holds).
- **Delete path:** reuse the existing recycle-bin delete + undo (CPE-033/044); menus follow CPE-748 icons +
  the MENUS.md standard.

## Child tickets
1. **CPE-749** — Backend `dir_children_sizes` (per-child recursive size, reusing `dir_size`; cancellable;
   cargo-tested). **Foundation.**
2. **CPE-750** — "Size (recursive)" sortable details column (lazy per-visible-row via `dir_size`, cached).
   Independent of 749; headless cache/sort logic.
3. **CPE-751** — Treemap/sunburst Space view with drill-down + largest-items (SVG, zero-dep). **Attended GUI.**
   *(prereq: 749)*
4. **CPE-752** — Reveal/delete actions from the treemap (reuse recycle-bin delete + undo). *(prereq: 751)*

## Work Log
2026-07-19 — Activated (autonomous, user away/PM-delegated). Researched the backend: `dir_size` +
`folder_stats` already provide recursive size, so re-scoped from "new streaming size walker" to a per-child
breakdown command + frontend surfaces. Decomposed into CPE-749–752 (backend foundation → column → treemap →
actions). Set In Progress.
