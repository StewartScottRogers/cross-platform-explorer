---
id: CPE-306
title: Agent process scoping & execution sandbox
type: Feature
status: Open
priority: High
component: Backend
estimate: 3-4h
created: 2026-07-13
---

## Summary

A coding agent launched in the console is arbitrary code that can touch the whole
filesystem and network with the injected credentials. Define and enforce the scope
of what a launched agent runs with: working directory, environment, and a clear
disclosure of the trust the user is extending. This is distinct from sidecar
isolation — it's about the *agent the sidecar spawns*.

## Acceptance Criteria

- [ ] Agent launched with cwd scoped to the chosen repo; env limited to the selected
      credential profile ([[CPE-279]]) plus the provider recipe ([[CPE-285]]).
- [ ] Clear pre-launch disclosure of what the agent can do (esp. "skip-permissions"
      style flags) and explicit user opt-in for dangerous modes.
- [ ] Where the OS allows, best-effort confinement (job objects / process groups) so
      the agent + its children are reaped together and counted against budget
      ([[CPE-297]]).
- [ ] Documented trust boundary in the threat model ([[CPE-304]]).

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-280]], [[CPE-279]]. **Phase:** C1 (design) → C2 (enforce).
**Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
