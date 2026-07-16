---
id: CPE-496
title: "Rich in-app conflict resolver (three-way / inline) for diverged sync"
type: Feature
status: Open
priority: Medium
component: Frontend
tags: [needs-prereq, big-design]
estimate: 4h+
created: 2026-07-16
epic: CPE-488
---

## Summary
Chosen conflict model (Q1): when a mirror sync diverges/conflicts, resolve it **in-app** rather than
only deferring to an external tool. Provide a three-way / inline conflict view (ours / theirs / base)
that stages resolutions and continues the merge/rebase — never losing work.

## Acceptance Criteria
- [ ] Conflicting files from a merge/rebase are listed with their state.
- [ ] A three-way / inline view lets the user pick or edit the resolution per hunk/file.
- [ ] Resolving stages the file and continues (or aborts) the merge/rebase cleanly.
- [ ] Work is never lost (abort restores the pre-sync state).
- [ ] Tests for the conflict-state parsing + resolution flow.

## Notes
**needs-prereq:** [[CPE-495]] (the mirror UI surfaces conflicts this resolves). `big-design` — the
heaviest v2 slice; sequence after CPE-495.
