---
id: CPE-034
title: Cut / Copy / Paste (Ctrl+X, Ctrl+C, Ctrl+V)
type: Feature
status: Done
priority: High
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

An in-app clipboard holding a set of paths plus a cut/copy mode; Paste performs a real copy or move
into the current folder.

## Acceptance Criteria

- [x] Ctrl+C / Ctrl+X stage the selection; Ctrl+V pastes into the current folder
- [x] Cut items render dimmed until the paste completes
- [x] Paste into the same folder auto-renames ("file - Copy.txt") rather than overwriting
- [x] Pasting into a subfolder of a cut source is rejected with a clear message
- [x] Errors are reported per item; the listing refreshes
- [x] Clipboard clears after a successful cut+paste

## Resolution

In-app clipboard (`lib/clipboard.ts`, 17 tests) holding paths plus a cut/copy mode. Cut items render
dimmed until the paste lands, as in Explorer.

The rules that matter, all tested:
- Pasting into the same folder **auto-renames** rather than overwriting.
- Pasting a folder into itself or a descendant is **refused**, with a clear message, before it ever
  reaches disk (the backend refuses it too — belt and braces).
- A cut+paste back into the source folder is a no-op and is refused rather than churning the file.
- The clipboard clears after a successful cut+paste, but not after a copy.

## Work Log

2026-07-11 — clipboard.ts written pure with 17 tests covering the self/descendant and same-folder cases explicitly.
2026-07-11 — Closing as Done.

## Notes
Never overwrite silently. Copying a folder into itself must fail loudly.
