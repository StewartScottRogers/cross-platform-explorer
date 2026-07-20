---
id: CPE-776
title: Rules editor UI + persistence for file coloring & labels
type: feature
status: Open
priority: low
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-709
estimate: 3-4h
---

## Summary
The user-facing rules editor for epic CPE-709: an ordered list of rules (condition → color/label), add /
edit / remove / enable-disable / reorder, with a live preview, persisted as a global rule set in settings.

## Acceptance Criteria
- [ ] Users create/edit/remove/reorder/enable rules across the CPE-774 condition kinds; live preview.
- [ ] The rule set persists in settings and reloads on startup; menus follow MENUS.md + CPE-748 icons.
- [ ] `npm run check` + suite green; GUI-verified.

## Notes
Prereq: CPE-774, CPE-775. Attended GUI. Persistence via the existing settings module.
