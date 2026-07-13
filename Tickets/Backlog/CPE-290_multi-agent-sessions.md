---
id: CPE-290
title: Multi-agent sessions / tabs
type: Feature
status: Open
priority: Medium
component: Frontend
estimate: 3-4h
created: 2026-07-13
---

## Summary

Run several agent sessions at once — tabbed/split consoles, each its own PTY,
agent, provider, and cwd — so you can compare agents or run one per repo
concurrently.

## Acceptance Criteria

- [ ] Multiple concurrent console sessions with independent PTYs and lifecycles.
- [ ] Tab/split UI to switch and manage sessions; per-session title (agent+model).
- [ ] Closing a session cleans up its process; no cross-session leakage.
- [ ] Bounded resource use; sessions survive pane hide/show.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-280]], [[CPE-289]]. **Phase:** C5. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
