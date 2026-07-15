---
id: CPE-438
title: "Two-way sync / mirror engine"
type: Feature
status: Open
priority: High
component: Backend
tags: [big-design]
estimate: 3-4h
created: 2026-07-15
epic: CPE-429
---

## Summary
The interconnect core (CPE-429): keep a local mirror and its remote in sync BOTH directions - pull +
push with divergence + conflict handling; never silently loses work. Safe-by-default (D3), per-repo
override.

## Acceptance Criteria
- [ ] Sync = fetch + fast-forward/merge/rebase (policy) then push; detect divergence and surface
      conflicts rather than clobbering.
- [ ] Dry-run/preview (ahead/behind, conflicts).
- [ ] Scheduled + on-demand; per-repo policy.
- [ ] Pure sync-plan logic unit-tested (ahead/behind/dirty -> planned actions).
