---
id: CPE-289
title: Agent session launcher UI (agent × provider × model)
type: Feature
status: Open
priority: High
component: Frontend
estimate: 3-4h
created: 2026-07-13
---

## Summary

The control surface that ties it together: pick an **agent × provider × model ×
credential profile**, see install status, and launch a console session in the open
repo. The combinatorial matrix from the reference, made a first-class UI.

## Acceptance Criteria

- [ ] Choose agent (with install state), provider, model, and credential profile.
- [ ] Launch composes the env via the routing engine ([[CPE-285]]) and opens a PTY
      console ([[CPE-280]]) in the current repo.
- [ ] Offers install/update inline when an agent isn't installed ([[CPE-282]]).
- [ ] Remembers last-used selections per repo.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-280]], [[CPE-285]], [[CPE-281]]. **Phase:** C5.
**Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
