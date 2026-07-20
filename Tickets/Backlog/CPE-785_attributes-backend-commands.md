---
id: CPE-785
title: Backend commands for permissions / attributes / timestamps
type: feature
status: Open
priority: medium
component: Backend
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-710
estimate: 2-3h
---

## Summary
Backend for the attributes editor (epic CPE-710): async Tauri commands to edit the platform's real model —
POSIX `set_permissions(path, mode)` (chmod), Windows attribute toggles (hidden/read-only/system/archive),
and `set_file_times(path, modified?, accessed?)` — each returning the prior state so the UI can offer undo.

## Acceptance Criteria
- [ ] POSIX chmod sets the mode; Windows toggles set/clear the attribute; timestamps are set.
- [ ] Each command is async (spawn_blocking) and returns the prior value for undo; errors cleanly.
- [ ] cargo-tested where feasible (mode round-trip on unix; attribute round-trip on windows) on the CI matrix.

## Notes
Prereq: CPE-784 (mode model). Take-ownership via the existing run_as_admin. Wired through lib/invoke.
