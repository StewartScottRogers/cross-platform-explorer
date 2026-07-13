---
id: CPE-306
title: Agent process scoping & execution sandbox
type: Feature
status: Done
priority: High
component: Backend
estimate: 3-4h
created: 2026-07-13
closed: 2026-07-13
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

## Resolution

Implemented `scope` in ai-console: `build_launch(AgentLaunchRequest)` composes the scoped `PtyLaunch` (without spawning, so it's testable) — the agent's run command, args = run args + provider-recipe args + user extra args, **cwd pinned to the chosen repo**, and env = routing-recipe env merged with the credential-profile env (nothing the session didn't ask for). `dangerous_flags()` surfaces known 'trust me' flags (--yolo, --dangerously-skip-permissions, --force, …) so the UI can require explicit opt-in. Ties routing ([[CPE-285]]) + vault ([[CPE-279]]) + PTY ([[CPE-280]]) into one scoped launch. 3 tests (cwd+env scoping, unsupported-provider error, dangerous-flag detection). 53 crate tests + clippy green.

**Deferred:** OS-level confinement (job objects / process groups reaping the agent + its children) rides with resource governance [[CPE-297]]; the pre-launch disclosure UI is [[CPE-289]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
2026-07-13 — Implemented scoped launch composition + dangerous-flag disclosure during dayshift. Done.
