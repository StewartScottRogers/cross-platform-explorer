---
id: CPE-437
title: "Local repo discovery + status"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [ready]
estimate: 1-2h
created: 2026-07-15
closed: 2026-07-15
epic: CPE-429
---

## Summary
Discover local git repos and report status (branch, ahead/behind, dirty) for the left-pane section
(CPE-429/435).

## Acceptance Criteria
- [x] Find git repos under configured roots; report branch + ahead/behind + dirty via git.
- [x] Pure parsing of git status --porcelain=v2 --branch unit-tested.
- [x] Skips unreadable dirs; bounded scan.

## Work Log
2026-07-15 - Nightshift. Added sidecar/repos/src/status.rs: RepoState (branch/upstream/ahead/behind/dirty, diverged()/up_to_date()) + pure parse_status() for `git status --porcelain=v2 --branch` (resilient to unknown lines). 4 unit tests, clippy clean. This is the input to the CPE-438 sync planner. (The filesystem discovery + bounded scan is a thin shell-out wrapper added with the sidecar process, CPE-432.)
