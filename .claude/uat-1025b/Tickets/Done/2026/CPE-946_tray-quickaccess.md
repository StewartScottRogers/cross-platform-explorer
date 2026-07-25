---
id: CPE-946
title: Tray quick-access list (pinned + recent)
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-713
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
First headless slice of the tray-resident epic (CPE-713). `cpe_server::tray_quick`:
- `QuickAccess::new(max_recent)` holding `QuickEntry { path, label, pinned }`.
- `touch(path,label)` (move-to-front recent, dedup, cap), `pin`/`unpin` (pinned persist at top; unpin
  restores as a recent), `remove`, and `items()` → pinned-first then recents (most-recent first).

Pure state model; the tray renders `items()`.

## Acceptance Criteria
- [x] Recents move-to-front, dedup, cap; pinned survive the cap and sit first; unpin restores as recent.
- [x] remove clears from both lists. 4 unit tests; clippy clean.

## Work Log
- 2026-07-23 (dayshift) — Activated CPE-713 with the quick-access list model. The actual tray icon/menu,
  minimize-to-tray, and background quick-launch are the remaining children.
