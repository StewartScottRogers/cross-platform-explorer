---
id: CPE-436
title: "Clone a remote repo to a local path"
type: Feature
status: Open
priority: Medium
component: Backend
tags: [ready]
estimate: 2h
created: 2026-07-15
epic: CPE-429
---

## Summary
Clone any accessible remote repo locally (CPE-429), shelling out to git per the provider manifest (D2).
Foundation for two-way mirroring.

## Acceptance Criteria
- [ ] Clone via the provider git URL to a chosen local path; progress + completion surfaced.
- [ ] Auth injected from the secrets broker (CPE-439); never on disk/logs.
- [ ] Actionable errors; a partial clone is cleaned up.
