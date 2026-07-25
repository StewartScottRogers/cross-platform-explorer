---
id: CPE-674
title: Extract an archive entry by dragging it out
type: feature
component: Multiple
priority: low
status: Deferred
tags: deferred-internal
created: 2026-07-18
epic: CPE-661
estimate: 2-3h
---

## Summary
Carved out of CPE-673. When viewing inside an open archive, dragging a file/folder entry out onto a real
folder (a file-list folder row or sidebar place) should **extract just that entry** to the drop target —
the natural "drag it out of the zip" gesture. Deferred because archive rows carry synthetic in-zip paths
(not real fs paths), so it needs a dedicated extract-to-path flow and can only be verified by actually
dragging out of an archive (GUI-critical). CPE-673 made archive DnD cleanly inert (non-draggable rows /
no drop targets) as the safe interim, so nothing is broken meanwhile.

## Acceptance Criteria
- [ ] In archive view, dragging an entry onto a real folder extracts that entry (and its subtree) there,
      via a backend extract-entry-to-path command; progress through the transfer/extract path.
- [ ] Re-enable `canDrag` for archive rows only for drag-OUT (still no drop-IN onto archive rows).
- [ ] cargo + `npm run check` + suite green; live GUI verification of the drag-out-extract gesture.

## Deferred
deferred-on: GUI-critical verification (dragging out of a real archive) + a small backend extract-entry
command. revisit-when: attended session / alongside CPE-672 drag-out work (related native-drag concerns).
