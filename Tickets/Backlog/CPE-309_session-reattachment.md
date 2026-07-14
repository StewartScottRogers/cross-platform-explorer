---
id: CPE-309
title: Session reattachment across sidecar restart
type: Feature
status: Open
priority: Medium
component: Backend
tags: [big-design]
estimate: 3-4h
created: 2026-07-13
---

## Summary

A long-running agent session shouldn't die because the AI Console sidecar was
restarted (crash, update, or user toggle). Decide and implement how running PTY
sessions survive: either the PTY-owning process outlives the sidecar UI process and
re-attaches, or sessions checkpoint and resume cleanly.

## Acceptance Criteria

- [ ] A running agent session survives a sidecar restart, or fails gracefully with
      the transcript preserved — never a silent kill of the user's work.
- [ ] Reattach restores live I/O to the console UI; state is reconciled ([[CPE-299]]).
- [ ] Interaction with resource budgets ([[CPE-297]]) and reaping is defined.
- [ ] Tested: restart the sidecar mid-session, assert the session is recoverable.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-280]], [[CPE-265]]. **Phase:** C2/C3. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
2026-07-14 — Triage while working the backlog. Two findings: (1) **Live reattach of a *running*
agent across a sidecar restart is architecturally impossible as built** — the agent PTYs are
children of the sidecar process, so they die when it does. True live reattach would require
re-parenting the agent to a supervisor that outlives the UI process (a large change) — that is the
real `big-design` core of this ticket. (2) The *achievable* value — sessions + transcripts survive
a restart and are relaunchable — is deliverable now: `history.rs` (CPE-292) already implements the
persistence, but it was **never wired in**. Split that implementable slice to **CPE-370** (`ready`).
This ticket stays `big-design` for the live-reattach core; revisit if PTY re-parenting is pursued.
