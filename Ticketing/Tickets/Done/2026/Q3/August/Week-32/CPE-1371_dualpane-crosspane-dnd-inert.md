---
id: CPE-1371
title: "Dual-pane: drag-and-drop between panes is inert (core commander gesture missing)"
type: Bug
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-617
created: 2026-08-06
---

## Problem (drag-drop audit, BUG 2)

Pane B (`ExplorerPane` at App.svelte) is mounted with `canDrag={false}`, has NO `on:drop` handler, and does
not `bind:draggedPaths`. So: dragging a row FROM pane B doesn't start (rows aren't draggable), and dropping
onto pane B does nothing (its `validTarget` returns false; no drop event is wired). The hallmark two-pane
"copy/move A↔B by dragging" doesn't exist. (Inconsistency: OS-external drops DO land in pane B folders via
`folderUnderCursor`, since those rows carry real `data-drop-path`.)

## Fix direction

Wire pane B like pane A: `canDrag` enabled, `bind:draggedPaths`, an `on:drop` that routes to `dropInto`
targeting pane B's folder (or the dropped-on subfolder), respecting the same move-vs-copy/self-descendant
guards. Add a cross-pane DnD test. (If cross-pane DnD is deliberately out of scope, close as wontfix with a
note — but from the user's seat it's a silent no-op in a mode where it's expected.)
