---
id: CPE-786
title: Attributes / permissions / timestamps editor dialog
type: feature
status: Open
priority: medium
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-710
estimate: 3-4h
---

## Summary
The editor UI for epic CPE-710: a per-OS dialog — POSIX mode grid (owner/group/other rwx + octal, via
CPE-784) vs. Windows attribute toggles — that batch-applies changes (with a preview) via CPE-785 and
registers undo for reversible edits. Launched from Properties / context menu.

## Acceptance Criteria
- [ ] Single-file and multi-select batch edits of permissions/attributes/timestamps, with a change preview.
- [ ] Each OS shows its native model; reversible changes are undoable; no regression to read-only Properties.
- [ ] check + suite green; GUI-verified.

## Notes
Prereq: CPE-784, CPE-785. Attended GUI. Recursive apply + full Windows ACL editing are follow-ups.
