---
id: CPE-305
title: Console ↔ Agent Watch integration
type: Feature
status: Open
priority: Medium
component: Multiple
tags: [needs-prereq]
estimate: 3-4h
created: 2026-07-13
---

## Summary

The end-to-end payoff: the AI Console **drives** an agent while Agent Watch
**observes** the filesystem activity it produces. Launching an agent session in the
console should be able to light up Agent Watch on that repo, so you see reads/writes
/edits/deletes as the agent works. Two features, one workflow.

## Acceptance Criteria

- [ ] Starting a console session can auto-enable Agent Watch scoped to the session's
      repo (opt-in, remembered).
- [ ] Integration goes through host-brokered channels only — no direct sidecar↔mode
      coupling (respects the boundary; Agent Watch keeps its observe-only rule).
- [ ] Session end returns Agent Watch to its prior state.
- [ ] Works whether Agent Watch is a host mode or itself a sidecar (decide + note).

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-280]], AGENT-WATCH.md work. **Phase:** C5. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during epic-plan hardening. Connects the two AI features.
