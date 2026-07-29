---
id: CPE-043
title: Drag and drop onto sidebar nodes
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Closes the acceptance criterion left explicitly unmet in CPE-040. Drag & drop currently only accepts
drops on folder rows in the list. Explorer also lets you drop onto the navigation pane — that is how
you file something into Documents without navigating there first, which is most of the value.

## Acceptance Criteria

- [x] Dropping a dragged selection onto a sidebar place/drive/child folder moves it
- [x] Ctrl+drag onto a sidebar node copies instead
- [x] Only valid targets highlight; dragging a folder onto itself or a descendant is refused
- [x] Reuses the CPE-030 backend commands, inheriting the same collision and safety rules
- [x] Moves performed this way are undoable (CPE-041)

## Resolution

Sidebar places, drives, and lazily-loaded child folders are now drop targets. Dropping a dragged
selection onto one moves it; Ctrl+drag copies. This is the part of drag & drop that carries most of
the value — filing something into Documents without navigating there first.

The validity rule is enforced at the target, not after the fact: a node only accepts (and only
highlights) if it is not one of the dragged items and not inside one of them. Dropping a folder onto
itself or its own descendant is therefore visibly impossible rather than rejected with an error.

Drops reuse `dropInto()`, so they go through the same `move_entries` / `copy_entries` commands as
paste and drag-within-list — inheriting the identical collision, auto-rename and self/descendant
rules, and the same undo recording. There is exactly one code path for moving files, not three.

Verified: svelte-check 0/0, 80 frontend tests, all four CI jobs green.

## Work Log

2026-07-11 — Filed because CPE-040 was closed with this criterion explicitly unticked rather than quietly dropped. The user asked for it, so it got done.
2026-07-11 — Routed sidebar drops through the same dropInto() used by list drops and paste. One code path for moving files means one place for the safety rules to live.
2026-07-11 — Closing as Done.

## Notes
Filed because CPE-040 was closed with this criterion unticked rather than quietly dropped.
