---
id: CPE-517
title: "Swarm — coordinator dispatch loop (mission → role tasks → collect)"
type: Feature
status: Open
priority: Medium
component: Multiple
tags: [needs-prereq, big-design]
estimate: 4h+
created: 2026-07-16
epic: CPE-502
---

## Summary
The heart of Swarm ([[CPE-502]], wave 2): a **Coordinator** turns one mission into role-assigned tasks,
**acquires file-ownership locks** ([[CPE-514]]) per task, dispatches Builders/Scout/Reviewer per the
team manifest ([[CPE-515]]), coordinates them via the mailbox ([[CPE-516]]), and collects results.

## Acceptance Criteria
- [ ] One mission decomposes into tasks with per-task file-glob ownership; concurrent tasks never overlap.
- [ ] Tasks dispatch to role agents (from the manifest) as Agent-Grid sessions; progress is visible.
- [ ] The coordinator sequences shared-dependency tasks and reassigns/halts via its authority (CPE-519).
- [ ] End-to-end: a small mission runs a coordinator + ≥1 builder to a collected result.
- [ ] Tests for the decompose→assign→collect logic (pure parts) + a scripted E2E where feasible.

## Notes
Wave 2 of [[CPE-502]]. **needs-prereq:** [[CPE-514]] (locks), [[CPE-515]] (roles), [[CPE-516]] (mailbox).
`big-design`. Not in SPR-01 (wave-1 foundation first).
