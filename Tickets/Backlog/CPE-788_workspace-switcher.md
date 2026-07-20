---
id: CPE-788
title: Workspace switcher UI (save / switch / rename / delete)
type: feature
status: Open
priority: medium
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-708
estimate: 2-3h
---

## Summary
The workspace switcher for epic CPE-708: save the current tabs/views as a named workspace (CPE-787), and
switch / rename / delete from a menu; selecting one applies its tabs. Persisted in settings.

## Acceptance Criteria
- [ ] Save current window state as a named workspace; switch applies its tabs (path + view/sort/filter).
- [ ] Rename/delete work; persists across sessions; menus follow MENUS.md + CPE-748 icons.
- [ ] check + suite green; GUI-verified.

## Notes
Prereq: CPE-787. Attended GUI. Reuse tab + settings persistence.
