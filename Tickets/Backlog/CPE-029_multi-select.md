---
id: CPE-029
title: Multi-selection (Ctrl+click, Shift+click, Ctrl+A)
type: Feature
status: Open
priority: Critical
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Selection is a single index. Every file operation (copy, cut, delete, properties) acts on a
*selection*, so multi-select is the foundation the rest of the file-operation work stands on.

## Acceptance Criteria

- [ ] Click selects one; Ctrl+click toggles an item in/out of the selection
- [ ] Shift+click selects a contiguous range from the anchor
- [ ] Ctrl+A selects all; Escape clears
- [ ] Status bar shows "N items selected" and the combined size
- [ ] Details pane shows a multi-selection summary rather than one file
- [ ] Selection logic is a pure, unit-tested module

## Resolution
## Work Log
## Notes
Blocks CPE-030..035.
