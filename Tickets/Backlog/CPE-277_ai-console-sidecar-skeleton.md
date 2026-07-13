---
id: CPE-277
title: AI Console sidecar skeleton (contract impl + empty pane)
type: Task
status: Open
priority: High
component: Multiple
estimate: 2-3h
created: 2026-07-13
---

## Summary

Stand up the AI Console as a real sidecar: its own crate/binary + its own frontend,
implementing the contract handshake, declaring the capabilities it needs (context,
secrets, storage, events), and rendering an empty pane through the host mount. Does
nothing useful yet — it proves the tenant boundary before any feature.

## Acceptance Criteria

- [ ] `sidecars/ai-console` process implements the contract; handshakes and reaches
      Ready under the supervisor.
- [ ] Serves its own minimal UI, embedded via the host UI mount ([[CPE-271]]).
- [ ] Declares its capability set; runs with no access it didn't request.
- [ ] Delete-test still green ([[CPE-272]]); explorer builds without it.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-260]] (Platform P4). **Phase:** C1. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
