---
id: CPE-789
title: Launch-time workspace auto-restore (graceful missing paths)
type: feature
status: Open
priority: low
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-708
estimate: 2h
---

## Summary
Auto-restore for epic CPE-708: persist the last session and reopen it on launch, using `pruneMissing`
(CPE-787) to drop moved/absent paths so restore never fails. No change to default single-tab startup when
unused.

## Acceptance Criteria
- [ ] Last session reopens on launch; missing/moved paths are skipped, valid tabs restored.
- [ ] Default single-tab startup is unchanged when the feature is off/empty.
- [ ] check + suite green; GUI-verified.

## Notes
Prereq: CPE-787. Attended GUI. Opt-in / off by default to preserve predictable startup (PURPOSE.md).
