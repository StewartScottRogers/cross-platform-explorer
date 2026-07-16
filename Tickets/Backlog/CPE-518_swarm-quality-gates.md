---
id: CPE-518
title: "Swarm — quality gates before a task is 'done'"
type: Feature
status: Open
priority: Medium
component: Multiple
tags: [needs-prereq]
estimate: 2-3h
created: 2026-07-16
epic: CPE-502
---

## Summary
A Swarm task ([[CPE-502]]) isn't **done** until it passes a **quality gate** (tests / review), so the
swarm can't mark broken work complete. Define gates + enforce them in the coordinator's close path.

## Acceptance Criteria
- [ ] A gate is definable per task/team (e.g. "tests pass", "reviewer approves") and runs before close.
- [ ] A failed gate keeps the task open + routes it back (to the builder / a reviewer) via the mailbox.
- [ ] Gate outcomes are visible per task; a mission completes only when all gates pass.
- [ ] Tests for gate evaluation + the fail-reopen path.

## Notes
Wave 2 of [[CPE-502]]. **needs-prereq:** [[CPE-517]] (coordinator close path). Not in SPR-01.
