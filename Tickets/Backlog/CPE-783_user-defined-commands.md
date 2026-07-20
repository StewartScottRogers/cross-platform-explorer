---
id: CPE-783
title: User-defined templated commands (toolbar / context-menu / palette)
type: feature
status: Open
priority: medium
component: Multiple
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-711
estimate: 4h+
---

## Summary
The user-command layer for epic CPE-711: define templated commands (CPE-781) and bind them to the toolbar,
context menu, and palette; run them over the current selection via a backend exec command, **confirming
before launching any external process**. Commands persist in settings.

## Acceptance Criteria
- [ ] Users define/edit/remove templated commands and choose where they surface; they run over the selection.
- [ ] External processes are confirmed before launch; no regression to built-in actions; persistence works.
- [ ] `npm run check` + suite green; GUI-verified.

## Notes
Prereq: CPE-781. GUI + a backend `run_command`-style exec (async, spawn_blocking; confirm in the UI first).
Menus follow MENUS.md + CPE-748 icons.
