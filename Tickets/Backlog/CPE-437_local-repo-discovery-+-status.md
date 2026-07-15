---
id: CPE-437
title: "Local repo discovery + status"
type: Feature
status: Open
priority: Medium
component: Backend
tags: [ready]
estimate: 1-2h
created: 2026-07-15
epic: CPE-429
---

## Summary
Discover local git repos and report status (branch, ahead/behind, dirty) for the left-pane section
(CPE-429/435).

## Acceptance Criteria
- [ ] Find git repos under configured roots; report branch + ahead/behind + dirty via git.
- [ ] Pure parsing of git status --porcelain=v2 --branch unit-tested.
- [ ] Skips unreadable dirs; bounded scan.
