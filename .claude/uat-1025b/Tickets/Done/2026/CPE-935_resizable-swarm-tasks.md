---
id: CPE-935
title: Make the Swarm Tasks pane drag-resizable down into the console
type: feature
component: Sidecar
priority: medium
tags: ready
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
The swarm-tasks textarea only had the tiny native corner resize grip (hard to find, next to Start).
Added a **full-width drag handle** below the textarea: drag it down and the tasks view grows **into the
console** (the taller textarea grows the toolbar, which shrinks the terminal below); drag up to shrink.
Clamped to a 30px floor and `innerHeight − 220` ceiling so some console stays visible. Native corner
resize disabled in favour of the clearer full-width handle.

## Acceptance Criteria
- [x] A full-width handle at the bottom of the tasks pane resizes the textarea by dragging.
- [x] Growing it expands into the console area; shrinking returns space. Floor/ceiling clamped.
- [x] Browser-verified drag; launcher harness + full suite green.

## Work Log
- 2026-07-23 — Added .swarm-resize handle + mousedown/move/up drag; verified a drag grows the pane into the console.
