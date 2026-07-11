---
id: CPE-034
title: Cut / Copy / Paste (Ctrl+X, Ctrl+C, Ctrl+V)
type: Feature
status: Open
priority: High
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

An in-app clipboard holding a set of paths plus a cut/copy mode; Paste performs a real copy or move
into the current folder.

## Acceptance Criteria

- [ ] Ctrl+C / Ctrl+X stage the selection; Ctrl+V pastes into the current folder
- [ ] Cut items render dimmed until the paste completes
- [ ] Paste into the same folder auto-renames ("file - Copy.txt") rather than overwriting
- [ ] Pasting into a subfolder of a cut source is rejected with a clear message
- [ ] Errors are reported per item; the listing refreshes
- [ ] Clipboard clears after a successful cut+paste

## Resolution
## Work Log
## Notes
Never overwrite silently. Copying a folder into itself must fail loudly.
