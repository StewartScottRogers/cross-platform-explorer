---
id: CPE-706
title: "EPIC: Disk usage & space analyzer"
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
